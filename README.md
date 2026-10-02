# cmatrix-rs

Matrix digital rain for Windows Terminal. No dependencies.

Inspired by [cmatrix](https://github.com/astyfoo/cmatrix) by Chris Allegretta, Abishek V Ashok and Xylia Allegretta. This is a separate Rust implementation; it contains no cmatrix code or data.

## Build

```sh
cargo build --release   # produces cmatrix.exe
cargo test
```

## Usage

```text
cmatrix [-hrsV] [--ascii] [-C color] [-M message] [-u delay]
  -C color    green (default), red, blue, yellow, cyan, magenta, white
  -M message  show a message in the centre of the screen
  -r          rainbow mode
  -s          screensaver: exit on the first key press
  -u delay    0 (fast) to 10 (slow), default 4
  --ascii     ASCII glyphs, for fonts without half-width katakana
```

Keys: `q`, `Esc`, `Ctrl+C` quit; `p` pause; `r` rainbow; `0`-`9` speed; `!` red, `@` green, `#` yellow, `$` blue, `%` magenta, `^` cyan, `&` white.

## Behaviour

- Size comes from the visible console viewport (`srWindow`) and is re-read every frame. Any size works, including 1x1, live resizing and font zoom. Rain in the overlapping area is kept; lanes added by a resize start mid-fall, so no region stays empty.
- Each drop has a white leading glyph. The glyphs it leaves behind stay in place, fade from light green to dark green, and change occasionally.
- One lane per even column. Each lane has its own speed; trail lengths vary.
- Animation is time-based at 60 fps. Only changed cells are written. Frames are wrapped in synchronized output (DEC private mode 2026), so Windows Terminal presents each frame whole. Terminals without that mode ignore it.
- 24-bit colour on a black background. Original console modes, cursor, autowrap and the main screen are restored on exit, including Ctrl+Break and panics (via `Drop` during unwinding).

## Differences from cmatrix

- Uses the Win32 console API and VT sequences directly instead of ncurses/PDCurses.
- cmatrix on Windows takes its size from the screen buffer (`dwSize`, which includes scrollback in conhost) and never handles resizes.
- Not carried over: Linux console fonts (`-l`, `-x`, `-f`), `-t` tty, `-L` lock, `-p`/`-P` preallocation, `-b`/`-B`/`-n` bold, `-a`/`-A` async, `-o` old scroll, `-m` lambda.

## Requirements

Windows 10 or later. Windows Terminal is the target and the only console tested; legacy conhost should work through its VT support but is untested. Katakana need a font that has them, or font fallback (Windows Terminal falls back automatically). Otherwise use `--ascii`.

## License

[MIT No Attribution (MIT-0)](LICENSE). Copyright 2026 Marcin W. Dąbrowski.
