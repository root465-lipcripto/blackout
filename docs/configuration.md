# Configuration

This development version does not read a TOML configuration file. Its supported settings are intentionally limited to environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `XDG_CACHE_HOME` | `$HOME/.cache` | Base directory for the SHA-256 object cache. |
| `BLACKOUT_SANDBOX_IMAGE` | `rust:1.85-slim` | Already-present image used for Cargo build/test validation. |
| `BLACKOUT_PROBE_IMAGE` | `registry.fedoraproject.org/fedora:latest` | Already-present image used to run the network probe binary. |

Podman commands always use `--pull=never`. Use immutable image digests when the image identity must remain stable. Image selection, Podman settings, and host isolation are the operator's responsibility. A validated TOML config and XDG config/data/state paths are planned after the first integration cycle.
