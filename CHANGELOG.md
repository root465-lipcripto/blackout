# Changelog

## 0.1.0 - initial public release

- Added project marker discovery for Cargo, Python, Node.js, and Go.
- Added Cargo locked dependency vendoring into a separate prepared copy.
- Added rootless Podman network isolation probe and offline Cargo build/test commands.
- Added a SHA-256 content-addressed file cache.
- Added tar+Zstandard bundles, integrity verification, and safe restore checks.
- Added CI checks for dependency advisories, licenses, duplicates, and registry/Git sources.
- Added Linux security model and research notes.
- Published a Linux x86_64 GNU/Linux binary requiring glibc 2.34 or newer.

This is an early development release, not a stable production release. Python, Node.js, and Go support remains detection-only.
