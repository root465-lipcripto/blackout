# Adapter development

An ecosystem is supported only when BLACKOUT can discover it, prepare a dependency-complete copy, validate build and tests with network isolation, bundle and restore it, and detect corruption in integration tests.

## Current adapter boundary

The first release uses a small Rust module boundary instead of empty adapter crates. `project::scan` returns marker-based ecosystem detection. `sandbox::prepare` implements Cargo vendoring; `sandbox::validate` runs Cargo in Podman; the bundle module is ecosystem-neutral. Python, Node.js, and Go are inventory-only.

## Requirements for a new adapter

1. Identify lockfiles and toolchain files without following symlinks outside the authorized project root.
2. Copy the source to a new preparation directory. Never silently rewrite the user's source tree.
3. Resolve a complete local dependency set from configured package sources and preserve integrity metadata.
4. Declare the exact offline build/test command and its required tools.
5. Execute build hooks only in the rootless network-disabled sandbox, with timeout and resource bounds.
6. Add fixtures for success, missing dependencies, corruption, malicious archive paths, and offline network denial.
7. Add real prepare/build/test/bundle/restore integration coverage before marking the ecosystem supported.

Do not call a cache or package-manager offline option a network sandbox. Keep command arguments as separate process arguments; do not construct shell command strings from project data.
