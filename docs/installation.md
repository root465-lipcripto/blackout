# Installation

## Build from source

Install a stable Rust toolchain (Rust 1.85 or newer), then from the repository root:

```sh
cargo install --path . --locked
```

Confirm the binary and host runtime:

```sh
blackout --version
blackout doctor --json
```

Podman is required for `sandbox-check` and isolated `validate` commands. BLACKOUT does not install Podman or modify host security settings. The selected images must already be present locally because container runs use `--pull=never`.

## Container images

Preparation needs Cargo on the host and network access to the registries referenced by the project's lockfile. It does not execute build scripts. For isolated validation, choose and review a Rust image, pull it before disconnecting, and pin it by digest for repeatable selection. Set `BLACKOUT_SANDBOX_IMAGE` to that local tag or digest. The separate network probe uses `BLACKOUT_PROBE_IMAGE`, which needs a libc compatible with the BLACKOUT executable.

Example (choose and inspect the image according to your own policy):

```sh
podman pull docker.io/library/rust:1.85-slim
podman image inspect docker.io/library/rust:1.85-slim
BLACKOUT_SANDBOX_IMAGE=docker.io/library/rust:1.85-slim blackout validate test ./prepared-project
```

The command fails if the image is not present; it will not pull it.
