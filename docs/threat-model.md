# Threat model

## Assets

- Host credentials and personal files.
- Integrity and availability of a prepared project and its bundle.
- Correctness of recorded file digests and validation results.
- Host kernel and Podman runtime.

## Trust boundaries

Project source, Cargo build scripts, tests, registry packages, the selected container image, archive input, and container runtime are untrusted inputs. The user-selected image and Podman installation are part of the trusted computing base for container validation.

## Mitigations implemented

- Preparation copies regular files only, skips symlinks and common generated/secret paths, and does not execute project build scripts.
- Cargo requires a lockfile for preparation; `cargo vendor --locked` prevents lockfile updates.
- Container validation uses Podman rootless user namespaces, no network, read-only container root, dropped capabilities, no-new-privileges, bounded PIDs/CPU/memory, a timeout, and a single writable project mount. Host home, SSH agent, and container-engine sockets are not mounted.
- Container images are not automatically pulled.
- Bundle extraction allows only regular files and directories, checks normalized relative paths, rejects duplicate paths, caps per-file/total bytes and entry count, verifies hashes before writing, and creates the destination directory exclusively.
- Cache objects are content-addressed, rehashed on reuse, and committed through a same-filesystem hard link.
- No telemetry or background service is present.

## Remaining risks

- Container escape, kernel, Podman, and image vulnerabilities remain possible.
- Resource limits may not constrain every filesystem or decompression behavior on every host.
- Build scripts intentionally execute arbitrary project code during `validate`; source review is still required.
- The secret exclusion list is heuristic. A secret in an unfamiliar filename or source file may be bundled.
- The bundle manifest is not signed. An attacker able to replace both manifest and payload can create a self-consistent malicious bundle.
- Cache objects currently lack provenance metadata, project references, and garbage collection.
- Filesystem race attacks by another local process are not fully mitigated.
- Rootless Podman and `--network=none` behavior must be verified on the current host; unsupported backends are not silently substituted.

## Safe use

Inspect project contents and container images before validation. Keep backups of any valuable working tree before running third-party tests. Use BLACKOUT on prepared copies; validation writes compiler output into that copy. Treat reports as local evidence, not signed attestations.
