# Licenses and bill of materials

deckscribe is MIT. Everything compiled into it must allow that: permissive
licenses only. This page says how that is checked, what the checks cannot see,
and what goes out with each release.

## Policy

Allowed: MIT, Apache-2.0 (also with the LLVM exception), BSD-2-Clause,
BSD-3-Clause, ISC, Zlib, 0BSD, BSL-1.0, CC0-1.0, Unicode-3.0,
Unicode-DFS-2016, CDLA-Permissive-2.0 (the TLS root certificates in
`webpki-roots`).

Refused by not being listed: GPL, LGPL, AGPL, MPL and other copyleft;
source-available licenses (BUSL, SSPL); non-commercial terms (CC BY-NC); and any
crate without a license.

The list lives in [`deny.toml`](../deny.toml). Adding a license to it is a
decision, recorded in the commit that adds it.

## Checks

| Check | Command | When |
|---|---|---|
| Licenses, banned crates, crate sources | `cargo deny check licenses bans sources` | every pull request and push |
| Security advisories (RustSec) | `cargo deny check advisories` | every pull request, and weekly |
| Vendored C/C++ | manual review, below | every bump of `transcribe-cpp` |

`cargo deny` reads `Cargo.lock` with all features on, so optional dependencies
(transcription, descriptions) are covered too.

## What `cargo deny` cannot see

**C and C++ inside `transcribe-cpp-sys`.** The crate compiles vendored sources
that are not crates. Reviewed at 0.4.2 from the crate's `LICENSE` files and its
`THIRD-PARTY-LICENSES.md`:

| Component | License |
|---|---|
| transcribe.cpp | MIT |
| ggml | MIT |
| miniz | MIT |
| Silero VAD | MIT |

No GPL text appears in the vendored sources. On every bump of `transcribe-cpp`,
read the new `THIRD-PARTY-LICENSES.md` and search the sources for GPL before
merging.

**Things used at run time but not shipped:**

| What | License | How it is used |
|---|---|---|
| ffmpeg | LGPL or GPL, depending on the build | Called as a separate program. Not linked, not in release archives. The Nix package puts nixpkgs' `ffmpeg-headless` on its path, so its closure includes it as a separate program, with its source in nixpkgs. |
| Parakeet TDT 0.6B v2 | CC BY 4.0 (NVIDIA) | Downloaded on first run from a pinned revision. The download message credits it. |
| Vulkan loader | Apache-2.0 | Linked dynamically from the system. |
| Vision model | the user's | Reached over HTTP; deckscribe never downloads one. |

**Benchmark data.** LPM and M3AV are CC BY-NC-SA 4.0. Their annotations and
videos are never committed or shipped; `bench/` holds only the scripts and the
list of sample videos.

## What ships with a release

- `THIRD-PARTY-NOTICES.md`, generated with `cargo about generate`, with the
  license text of every crate compiled in, plus the four vendored components
  above. It goes in every release archive next to the binary.
- An SBOM in CycloneDX JSON, generated with `cargo cyclonedx`, attached to the
  GitHub release.
- Binaries built with `cargo auditable`, which embeds the dependency list in
  the binary so `cargo audit bin` can check a downloaded copy later.
