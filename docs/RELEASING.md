# Releasing

1. Set `version` in `Cargo.toml`, run `cargo build` to update `Cargo.lock`, and commit both.
2. Tag the commit and push it: `git tag v0.2.0`, then `git push origin master v0.2.0`.
3. The `release` workflow (`.github/workflows/release.yml`) runs on the tag. It fails unless the tag equals `v` plus the `Cargo.toml` version, then runs `cargo fmt --check`, clippy and the tests, builds `cmatrix.exe`, and publishes a GitHub release.

Release assets:

- `cmatrix-<version>-x86_64-pc-windows-msvc.zip`: `cmatrix.exe`, `README.md`, `LICENSE`.
- `cmatrix-<version>-x86_64-pc-windows-msvc.zip.sha256`: SHA-256 checksum, `sha256sum -c` format.

`cmatrix.exe` links the C runtime statically (`.cargo/config.toml`), so it runs without the Visual C++ redistributable.

The release is created by the last step, so a failed run publishes nothing. To redo a release, delete the release and the tag on GitHub, fix the problem, and tag again.
