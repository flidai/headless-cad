# Dependency license review

The project's own code is licensed under **Apache-2.0**. The workspace declares
that SPDX identifier and every project crate inherits it. [LICENSE](../LICENSE)
is the unchanged [canonical Apache License 2.0 text](https://www.apache.org/licenses/LICENSE-2.0.txt).
No copyright owner or author attribution has been invented.

## Direct Rust dependencies

Checked on 2026-10-11 against the versions resolved by [Cargo.lock](../Cargo.lock),
using `cargo metadata --format-version 1 --locked` and the corresponding locally
downloaded package `Cargo.toml` files. This includes direct normal, development,
build, and target-specific dependencies of all workspace crates; internal project
crates are omitted from the table.

| Dependency | Locked version | Declared SPDX license expression | Used by |
| --- | --- | --- | --- |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | cad-recipe |
| blake3 | 1.8.2 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | cad-assembly, cad-build, cad-cli, cad-kernel |
| cap-std | 4.0.3 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | cad-cli |
| clap | 4.6.7 | MIT OR Apache-2.0 | cad-cli |
| earcutr | 0.5.0 | ISC | cad-kernel; derived face triangulation |
| robust | 1.2.0 | MIT OR Apache-2.0 | cad-kernel; adaptive orientation checks |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | cad-cli, Linux target |
| serde | 1.0.229 | MIT OR Apache-2.0 | cad-assembly, cad-build, cad-kernel, cad-preview |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | cad-assembly, cad-build, cad-cli, cad-kernel, cad-preview, cad-recipe |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | cad-preview; SHA-256 identity for fetched display assets |
| starlark | 0.14.2 | Apache-2.0 | cad-recipe |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | cad-build, cad-cli |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | cad-assembly, cad-build, cad-export, cad-kernel, cad-preview, cad-recipe |

Direct dependencies use permissive licenses. Most offer Apache-2.0; `earcutr`
uses ISC. Its pinned package license was checked locally and is included in
[the earcutr notice](third-party-earcutr-license.txt), which is copied into the
local distribution with the documentation. Supporting triangulation does not
replace the original analytic kernel. Upstream licenses and attribution remain
in force; first-party code remains Apache-2.0. The pinned `robust` package
also includes its Apache/MIT license texts in this documentation bundle.
No copyleft-only dependency was added by this licensing change.

The local source packages contain top-level license files for each listed package
except `starlark`; its Apache-2.0 declaration was checked in its package manifest.
Its upstream license and applicable notices still need to be included when
assembling a distribution. This review records declarations, not a file-by-file
audit of dependency source or a completed redistribution notice bundle.

## Release inventory still required

Before distributing a binary or dependency source bundle:

1. Generate a complete dependency inventory from the release lockfile, enabled
   features, and each supported target. Record the chosen alternative for each
   multi-license expression.
2. Check transitive dependencies, bundled native code, build-time generators,
   and any copied assets; collect the applicable license texts and notices.
3. Include the project's Apache license and required third-party notices in the
   release package. Verify the contents of the final package, not only manifests.
4. Refresh this table whenever direct dependencies or their resolved versions
   change.

The resolved metadata contains 224 packages including seven workspace crates. That
count is an inventory starting point, not a claim that every package is shipped
or audited. For example, transitive `r-efi` 6.0.0 declares
`MIT OR Apache-2.0 OR LGPL-2.1-or-later`; its Apache-2.0 alternative is available,
so an LGPL term in an alternative expression does not by itself require choosing
LGPL. The release inventory must record the actual selection.

Optional Python/OCP interoperability checks and browser test tools are outside
this Rust dependency review. They are not CAD-kernel runtime dependencies; include
them in a separate inventory if their packages are redistributed.

The streamed-display increment adds `sha2` 0.11.0 and six resolved supporting
packages. Their local manifests offer MIT and Apache-2.0 alternatives; the upstream
[sha2 documentation](https://docs.rs/sha2/0.11.0/sha2/) confirms the dual license.
Browser verification uses the platform Web Crypto API rather than redistributed
JavaScript cryptography. This scoped check does not close the release inventory
requirements above.
