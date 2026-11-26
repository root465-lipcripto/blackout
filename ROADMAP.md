# Roadmap

## 0.1.x — foundation

- Complete Cargo bundle/build/test integration coverage across supported hosts.
- Add structured reports and cache provenance/reference metadata.
- Tighten archive fuzzing and timeout/cleanup behavior.
- Keep release artifacts disabled until the acceptance flow passes.

## 0.2 — additional ecosystems

- Python lockfile and wheelhouse preparation with isolated offline install/build/test.
- Improve image/toolchain selection and compatibility reporting.

## 0.3 — portability

- Go and Node.js adapters after end-to-end offline fixtures pass.
- ARM64 CI and package formats.
- Cache garbage collection with dry-run and explicit confirmation.

## Later

- Signed bundle attestations, multi-platform bundle validation, and stable format policy.
