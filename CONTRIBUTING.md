# Contributing

1. Open an issue for larger changes and describe the behavior and threat boundary.
2. Keep ecosystem support explicit: do not mark a backend supported until preparation and isolated build/test coverage pass.
3. Run `cargo deny check`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`.
4. Add tests for archive input, path handling, and failure paths when changing bundle code.
5. Do not include real credentials, private source, or generated bundles in pull requests.
6. Keep dependencies minimal and commit `Cargo.lock`.

By contributing, you agree that your contribution is licensed under GPL-3.0-only.
