# Current limitations

- Linux is the only target implemented and exercised.
- Rust/Cargo is the only preparation and offline validation adapter.
- Cargo projects with an existing `.cargo/config.toml` or `.cargo/config` are not prepared automatically.
- Cargo preparation requires a committed/generated `Cargo.lock` and a working Cargo installation while preparing.
- Project code is executed only by the explicit `validate build` or `validate test` commands, inside Podman. The selected container image must already exist locally.
- The local network check needs an already-present image and Podman rootless support.
- Python, Node.js, and Go are detected but not prepared or validated.
- The cache does not yet record origin, compatibility, or project references; there is no garbage-collection command.
- Bundle filtering is heuristic; review content before sharing.
- Checksums are not signatures. There is no signature/attestation mechanism.
- The only prebuilt binary is Linux x86_64 GNU/Linux and requires glibc 2.34 or newer. There is no ARM64 binary, Windows/macOS target, or reproducibility proof.
- No performance benchmarks or signed release attestations are provided.
