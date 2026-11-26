# Usage

## Inspect and prepare

```sh
blackout scan ./source --json
blackout prepare ./source --output ./source-prepared
```

`scan` reports marker files and ambiguity. Cargo is the only supported preparation backend. Preparation requires a lockfile, rejects a project-level `.cargo/config.toml` until its settings can be merged safely, copies only regular files, omits common secret/generated paths, and runs `cargo vendor --locked` in the copy. Review the result before carrying it to a disconnected machine.

## Validate offline

```sh
blackout sandbox-check
blackout validate build ./source-prepared
blackout validate test ./source-prepared
```

`validate` first probes the selected container runtime, then executes Cargo inside Podman with no network. It may update the prepared copy's `target/` directory. The project build scripts and tests execute in the container, so inspect source and image first.

## Bundle and restore

```sh
blackout bundle create ./source-prepared --output ./source.blackout
blackout bundle inspect ./source.blackout
blackout bundle verify ./source.blackout
blackout restore ./source.blackout ./restored-source
blackout report ./restored-source --json
```

Restore requires a directory that does not yet exist. Inspect and verify a bundle before moving it. See [the bundle format](bundle-format.md).

## Cache files

```sh
digest_line="$(blackout cache put ./source.blackout)"
blackout cache status
```

The `cache put` output includes a SHA-256 digest. Use its 64-character hex value with `blackout cache verify <digest>`. The cache is independent of Cargo's registry cache; cache garbage collection and dependency provenance are future work.

## Machine-readable output

`scan --json`, `doctor --json`, and `validate` output JSON suitable for scripting. Diagnostics go to stderr on errors. Do not treat a successful JSON serialization as proof that a requested build passed; inspect the `passed` and `network_isolated` fields emitted by `validate`.

`report` prints Markdown by default or JSON with `--json`. It records the last observed build/test results, selected image, elapsed time, and isolation result. These records are local, unsigned, and not bound to a fingerprint of the current source tree; the report calls out this limitation.
