# Design research

Research checked on 2026-11-26. This is a design comparison, not a security audit.

## Findings

| Source/tool | Confirmed capability | Design consequence |
|---|---|---|
| [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html) and [cargo vendor](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html) | Cargo supports offline mode and vendored dependencies. Offline mode uses local data and errors when required resources are missing. | Preparation vendors a locked project; isolated validation still disables container networking because package-manager offline flags alone do not constrain arbitrary build scripts. |
| [Go Modules Reference](https://go.dev/ref/mod) | `-mod=vendor` uses a project's vendor directory for build and test without network or module cache. | Go is detectable now; a complete adapter can rely on Go's vendor mode plus the same external network isolation. |
| [pip install](https://pip.pypa.io/en/stable/cli/pip_install/) | `--no-index --find-links` installs from local archives without querying package indexes. | Python preparation will need a closed local wheelhouse and explicit handling of source distributions/build backends. |
| [npm configuration](https://docs.npmjs.com/cli/using-npm/config/) | npm's `offline` setting controls cache use and can fail when cache entries are absent; `prefer-offline` can still refresh stale data. | A Node adapter must test lockfile installation under actual namespace isolation rather than treating cache preference as proof. |
| [Podman run](https://docs.podman.io/en/latest/markdown/podman-run.1.html) | The `none` network mode creates a network namespace without configured interfaces. | BLACKOUT requests this mode and attempts a TCP connection to a host-local listener before running project commands. |
| [OCI Image Layout](https://github.com/opencontainers/image-spec/blob/main/image-layout.md) | OCI layouts use content-addressed blobs and location-addressed references and can be transported in archives. | The object cache follows the content-addressing idea. `.blackout` is tar+Zstandard and is **not** an OCI image or OCI layout. |
| [Nix reproducible builds](https://reproducible.nixos.org/) | Nix derivations provide declared dependencies and sandboxing as a basis for reproducible builds; that alone does not eliminate all nondeterminism. | BLACKOUT will not label a successful offline build as reproducible. |
| [Bazel external dependencies](https://bazel.build/external/overview) | Bazel has explicit external dependency/repository mechanisms; repository rules are part of dependency acquisition. | A future adapter must distinguish prefetch from execution with network disabled. |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/checks/index.html) | Checks dependency advisories, licenses, duplicate crates, and source locations. | CI runs its advisory, license, and source checks from the committed lockfile and allowlist. |

## Similar-name check

A GitHub repository search for `blackout` and Rust returned several unrelated public projects, including a password manager and a monitor blanking application. A search for the longer phrase “offline development engine” returned no repository in the queried results. Search results are not a trademark or global name clearance. BLACKOUT is therefore presented with its specific subtitle and repository owner, without a uniqueness claim.

## Deliberate boundaries

- The tool does not claim a container alone is a complete security boundary.
- A local package cache or `--offline` flag is not evidence that arbitrary project commands cannot access the network.
- A hash is an integrity check against a known manifest, not an author signature.
- A successful build on one image/architecture does not establish reproducible output or compatibility elsewhere.
- `.blackout` is a project-defined format with a versioned JSON manifest, not OCI-compatible metadata.
