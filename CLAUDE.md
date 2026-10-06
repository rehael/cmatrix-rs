# cmatrix-rs: notes for agents

Read this file, `README.md` and `docs/RELEASING.md` before changing anything. This file holds what the code does not show: goals, decisions and their reasons, the agreed `-F` spec, the verification procedure, environment facts and open items. The global rules in `~/.claude/CLAUDE.md` still apply (authorship, git, verification).

## Goal and constraints

- Matrix digital rain for Windows Terminal, written from scratch in Rust. Inspired by cmatrix (<https://github.com/astyfoo/cmatrix>, GPL-3.0-or-later, a fork of abishekvashok/cmatrix 2.0). Not a port.
- Owner requirements: works in Windows Terminal at any size (tiny, huge, live resize, font zoom) and looks like the film: white leading glyph, stationary fading trails, mutating glyphs, half-width katakana.
- Windows only (`compile_error!` on other targets). No crate dependencies: Win32 calls are hand-declared FFI (foreign function interface) to kernel32. The owner prefers small binaries; ask before adding a crate and health-check it first.
- Licence MIT-0, copyright 2026 Marcin W. Dąbrowski. Never copy cmatrix code, data tables or text. The glyph set was changed from cmatrix's katakana list to the Unicode block for this reason. The README section "Why MIT-0 and not cmatrix's GPL-3.0" cites the legal basis.

## Repository and accounts

- GitHub: <https://github.com/rehael/cmatrix-rs>, branch `master`, remote `git@github.com:rehael/cmatrix-rs.git`. Push over SSH works on the owner's machine as configured.
- Do not use `gh` for this repo: on the owner's machine it is not logged in to the owning account.
- The repo is public. While public, the unauthenticated REST API works for runs and releases, for example `curl -s "https://api.github.com/repos/rehael/cmatrix-rs/actions/runs?per_page=5"`. If it becomes private, ask the owner to check the Actions tab.
- Commits use the repo-local `user.name = Marcin W. Dąbrowski`. Do not change git identity settings.
- Commit, push and tag only when the owner says so, each time. Conventional Commits. No attribution trailers.
- Machine-specific notes (local paths, tool locations) live in `CLAUDE.local.md`, which is git-ignored; read it when present. Never put account names, key names, email addresses or local paths into tracked files.
- History: `c31562a` initial, `1af8d15` release workflow, `c956168` licence rationale (tag `v0.1.0`), `14069d2` `-F` typewriter mode, `ddc2b46` version 0.2.0 (tag `v0.2.0`).

## Build and environment

- Rust 1.98.1, edition 2024. `cargo build --release`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` (default rustfmt settings).
- Local builds may use a redirected cargo target directory; find it with `cargo metadata --no-deps --format-version 1` (`target_directory`). CI uses the default `target/`.
- `.cargo/config.toml` links the C runtime statically (`+crt-static`), so the exe needs no Visual C++ redistributable. It imports only `KERNEL32.dll`, `api-ms-win-core-synch-l1-2-0.dll` and `ntdll.dll`. Check with `dumpbin -dependents` from the MSVC build tools; some `objdump` builds cannot read the exe.
- Installed copy: `cargo install --locked --path .` (currently 0.2.0).
- Markdown: `markdownlint-cli2` with the owner's shared config (location in `CLAUDE.local.md`) and `pandoc -f gfm -t native --fail-if-warnings <file>`. No hard-wrapped prose.
- The Bash tool's stdout is not a console: cmatrix run there stops with `cmatrix: stdout must be a console` (exit 1). Live runs go through Windows Terminal (see Verification).

## Architecture

One binary; each file in `src/` stays under 500 lines.

- `main.rs`: parses arguments, then reads the `-F` text before the console switches to raw mode, so errors print normally. Opens the console and runs a 60 fps loop (16.667 ms):
  - reads keys;
  - re-reads the window size every frame and resizes rain, screen and typer when it changes;
  - clamps `dt` to 0.1 s;
  - calls `rain.update(dt * speed_factor(delay))`, where `speed_factor = (11 - delay) / 7` (delay 4 is 1x);
  - calls `typer.update(dt, ready)`, where `ready = waiting && rain.reach(height / 2) >= 0.5`;
  - composes, flushes and sleeps. Pause (`p`) skips both updates.
- `console.rs`: Win32 console layer.
  - Output comes from `GetStdHandle` and must be a console; VT (virtual terminal) processing is enabled.
  - Input comes from `CreateFileW("CONIN$")`, so keys work when stdin is a pipe.
  - Raw input (no processed, line, echo or VT input), so Ctrl+C arrives as `'\x03'`. `SetConsoleCtrlHandler` sets a flag on Ctrl+Break or window close, and the loop then exits normally.
  - Size comes from `srWindow` (the visible viewport), not `dwSize`, which includes scrollback; that was cmatrix's Windows bug.
  - On start: alternate screen `?1049h`, hidden cursor `?25l`, autowrap off `?7l` so the bottom-right cell cannot scroll.
  - `Drop` writes the reverse sequence, restores the console modes and closes `CONIN$`; it also runs during a panic unwind. The `entered` flag prevents writing VT codes when VT could not be enabled. Compile-time asserts check the FFI struct sizes.
- `rain.rs`: simulation in cells and seconds.
  - One lane per even column (`width.div_ceil(2)`). Lane speed is 6 to 20 cells/s, re-rolled when the lane is empty.
  - A drop (`Stream`) writes a random glyph into each cell its head passes. Cells stay in place and lose brightness at `speed / trail` per second; the head cell is held at full brightness.
  - Trail length is `4 + rand(height * 3/4)` cells. The next drop spawns after `(trail + gap) / speed`, with gap `1 + rand(height)`. Visible glyphs mutate at 0.6 per second.
  - `resize` keeps the overlapping cells. Lanes added by a resize are seeded mid-fall with a pre-drawn trail (`seed`), so a grown window has no empty area; at startup lanes start empty and fill from the top, like cmatrix.
  - `reach(row)` is the fraction of lanes with a lit cell at or below `row`.
  - Glyphs: half-width katakana U+FF66 to U+FF9D, digits, `:."=*+-<>|`. `--ascii` uses `'!'..='z'`.
- `render.rs`: `Glyph { ch, rgb, bold, bg, ul }`.
  - `compose` fills the back buffer each frame: trails (`trail_rgb`: 32 brightness levels, a slight white glow near the head), heads (`head_rgb`: base colour 80% toward white, bold), then the `-F` box or the `-M` message.
  - `Screen::flush` compares the back and front buffers and sends only changed cells, in one `write_all` per frame wrapped in synchronized output (`?2026h` / `?2026l`). The cursor is moved only when the next changed cell is not adjacent.
  - Pen state (foreground with weight, underline, background) persists across frames and resets on clear. Clear is `SGR 0`, black background, `ED 2`; background colour erase gives a black canvas.
  - Two-cell characters: the right cell holds `WIDE_TAIL` (`'\0'`), which is never sent; cursor tracking advances by `char_width`.
  - Underline is sent as `4;58:2::R:G:B` (the colon form Windows Terminal documents) and switched off with `24`.
- `typer.rs`: the `-F` logic, pure and unit-tested.
  - `parse`: tabs become 4 spaces; control and zero-width characters are dropped; lines are trimmed and blank lines skipped.
  - `char_width`: 0 for combining and zero-width marks, 2 for East Asian wide characters and common emoji ranges, otherwise 1. An approximation that avoids a dependency.
  - `wrap`: breaks at spaces and hard-splits long words.
  - The state machine, and `TextFrame` for the renderer.
- `args.rs`: hand-written parser. Short flags combine (`-rs`); values are attached or separate (`-u2`, `-u 2`). `-C`, `-F`, `-M` and `-u` take values; `-F` with `-M` is an error. The `COLORS` table holds names, runtime keys and RGB values.
- `rng.rs`: SplitMix64, seeded from the clock.

## `-F` typewriter spec (agreed with the owner; keep it)

1. Rain only, until `rain.reach(height / 2) >= 0.5`. This happens once per run.
2. The box appears: 3 rows starting at `y0 = height / 2 - 1`, columns `2..width-2`, so 2 rain cells stay visible on each side. The rain inside is dimmed to 25% (`BOX_DIM`). Text starts at column 4 (2 rain cells plus 2 padding cells). Text width is `width - 9` (`MARGIN`: 8 cells around the text plus 1 for the cursor after the last character).
3. First line only: a block cursor, whose background is the rain colour of its column, blinks 300 ms on and 300 ms off for 5 s.
4. Typing, 50 ms per character:
   - the cursor is an underline in the rain colour;
   - the cell above it shows a random rain glyph at a random lightness (25% to 100% of the rain colour), re-rolled every frame;
   - the character then settles in bold white and the cursor moves on;
   - spaces scramble too and end blank, so the dimmed rain shows through.
5. The cursor disappears and the line fades from white to black over 5 s. Letters hide the rain behind them until the fade ends.
6. Following lines: after the fade, the block cursor blinks for 1 s, then typing starts. There is no 5 s blink.
7. After the last line has faded, the box disappears. After 10 s of rain the box returns and the sequence restarts at step 3.
8. Wrapping and resizing:
   - wrapped pieces are separate lines with the full cycle;
   - on resize the text is re-wrapped, and the current source line restarts in the 1 s cursor phase;
   - below 10 columns or 3 rows nothing is drawn and the sequence is frozen.
9. Input and keys:
   - `-F -` reads stdin to the end before the console opens. If stdin is a terminal: `-F - needs text piped to stdin`. Empty input: `<path>: no text to show` (exit 1).
   - `p` pauses the sequence; `0`-`9` change only the rain speed.
   - In rainbow mode the cursor and scramble take the hue of the cursor's column.

## Release pipeline

- `.github/workflows/release.yml` runs on tags `v*` on `windows-latest`. `actions/checkout` is pinned to commit `3d3c42e5aac5ba805825da76410c181273ba90b1` (v7.0.1).
- The default shell is bash. GitHub's docs say a pwsh step only fails on its last command's exit code, while bash runs with `-eo pipefail`.
- Steps:
  1. install stable Rust with clippy and rustfmt;
  2. the tag must equal `v` plus the version from `cargo pkgid`;
  3. fmt, clippy and tests (`--locked`), then the release build;
  4. pwsh packaging: the zip, plus a `.sha256` file in `sha256sum -c` format with LF line endings;
  5. `gh release create` with `GITHUB_TOKEN` and generated notes.
- To cut a release, follow `docs/RELEASING.md`: bump `Cargo.toml`, run `cargo build` to update `Cargo.lock`, commit, tag, push both. A run takes about one minute.
- Verified releases: v0.1.0 (run 37018599690) and v0.2.0 (run 37484153438). For each, the checksum matched, `-V` was correct, and the downloaded binary ran live.
- GitHub Packages has no Cargo registry, so the repo's "Packages" panel stays empty. Not done yet: crates.io, winget, Scoop, an aarch64 build.

## Verification

Definition of done for code changes: `cargo test --locked`, clippy with `-D warnings`, `cargo fmt --check`, a release build, and a live run in Windows Terminal that shows the change. For docs: markdownlint and the pandoc check.

Live runs use the tools in `.claude/live-test/` (usage is in the header of `wt-run.ps1`):

- `wt-run.ps1`:
  - opens a new Windows Terminal window running `cmd /k <command>` and optionally resizes it;
  - captures frames with `PrintWindow`, which works even when other windows cover it;
  - stops cmatrix with `conkey.exe` (a key written into its console input) or `sendbreak.exe` (Ctrl+Break);
  - captures `after.png`, then closes the window with `WM_CLOSE`.

  The helpers compile from their `.rs` files on first use.
- `sheet.py` stacks the middle band of many frames into one image, so a whole sequence can be read at once. For fast effects, measure the frames with Pillow first (for example, count white or green pixels in the box row) to pick the ones worth viewing.
- Sample `-F` input: `test.txt`, an original 8-line poem (archaic English with Polish and CJK) written for this repo. It covers all nine lowercase Polish diacritics (no uppercase), wide CJK characters, and a 169-character line that wraps in windows under about 178 columns. Keep test content original: no quotations from books or films.
- The README cover, `docs/cover.png`, is frame `011_13.7s.png` of a `wt-run.ps1` run: `-F test.txt`, 900x500 window, captures every 1 s. Regenerate it the same way.
- Never use SendKeys, or `SetForegroundWindow` followed by keystrokes: once, the keys went into the owner's Claude Code window.
- Windows Terminal applies its own launch size (here maximised, about 1936x1168 px) after the window appears, and `wt --size` was ignored. The script waits 1.5 s, restores the window, then resizes it.
- `powershell -File` cannot take arrays as parameters; pass strings.
- In `cmd /k "... & echo %errorlevel%"` the value expands before the command runs, so it does not show the real exit code.
- Error paths that run before the console opens (missing file, empty input, bad arguments) can be tested directly in the Bash tool.

## Facts checked during development

- Installed Windows Terminal: 1.24.11911.0. Synchronized output (DEC mode 2026) is supported since 1.23.20211, and coloured underlines (SGR 58, colon form `58:2::R:G:B`) since 1.20.
- Rust's `thread::sleep` on Windows uses high-resolution waitable timers (Windows 10 1803 and later), so 60 fps pacing works without `timeBeginPeriod`.
- `CreateFileW("CONIN$")` returns the console input buffer even when stdin is redirected (Microsoft Learn, "Console Handles").
- Half-width katakana (U+FF61 to U+FF9F) are one cell wide. Windows Terminal renders them through font fallback with the default font.

## Known limitations and open items

- `-M` counts characters, not cells, so wide characters misalign its box.
- Combining marks are dropped: decomposed (NFD) text loses its accents. Composed text, the norm on Windows, is fine.
- Not tested live: legacy conhost, console restore after a panic, `p` during `-F`, and resizing in the middle of a typed line. Unit tests cover the last two.
- Measured load at about 190x57 cells: 2% to 3% of one CPU core, 4.5 MB working set.
- Owner's working preferences: terse replies, verification backed by command output, and asking before commits, pushes, tags and new dependencies.
