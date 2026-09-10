# Prototype review and validation

Scope: [GPUI gate #3](https://github.com/ParthMmm/web2026/issues/3) and [Effect/Rust gate #4](https://github.com/ParthMmm/web2026/issues/4). Approved review baseline: `9d36f032cbbd58a0d0d80e12277f842708f04ef8`. Two fresh, read-only reviewers examined the complete staged prototype change, not just committed history. A second independent pair checked the fixes. Root/site changes outside the API TypeScript exclusion were excluded from the implementation diff.

## Standards

No documented-standard blockers were found. Two nonblocking judgment calls:

- **Primitive Obsession:** the desktop import guard compared display text with `Scanning…`. Fixed with explicit `scanning` state, reset on both success and failure.
- **Duplicated Code:** Rust and Python repeat the benchmark request schedule. A shared schedule fixture remains a follow-up. The current warm-hit and completion gates fail closed if the two diverge; no schedule abstraction was added solely for this prototype.

The follow-up reviewer found no new standards blockers in dependency checking, scanning state, display preflight, or the scoped TypeScript exclusion.

## Spec

One blocker was found: the required libvips helper was measured at 8.18.3 but not enforced. Fixed by checking the exact version before imports and public preview rendering, caching successful checks only, adding a rejecting-helper workflow regression, and documenting versioned Homebrew extraction. The archived installation procedure was not rerun; local validation used the installed 8.18.3 helper. The follow-up reviewer found this blocker resolved and no scope violations in the delta.

The source reviews do not replace performance validation. The final post-review warm GUI run failed the render-gap budget. **#3 stays blocked/open.** Its separate native browsing check and behavior tests pass. **#4's local and deployed contract proof passes**; GitHub CI execution remains pending publication.

Summary: standards — two initial nonblocking findings, one fixed and one deferred; spec — one initial blocker, fixed. No source-review blockers remain. The separate measured GPUI performance blocker remains unresolved.

## Validation

- Desktop: **9 workflow tests pass**, Cargo formatting/check/release build pass. Unsupported libvips rejection was observed failing before the fix and passing after it.
- Native UI: real pointer selection and FPS HUD toggle verified using app-owned Metal scene captures; Cmd-Q verified through System Events. Images remain local and ignored. Full accessible controls and keyboard grid navigation are future product work.
- Performance: six earlier 20/200/944 cold/warm runs passed. Final post-review 20-photo cold passed; warm failed at **1030.81 ms** maximum render gap, budget **250 ms**. Ten of ten previews completed. See [desktop methodology and results](../../apps/desktop/README.md). Failed runs were retained, not relabeled as passes.
- API: frozen Bun installation, scoped Ultracite lint/format, TypeScript checking, **12 HTTP/OpenAPI tests**, **3 Rust fixture tests**, and real loopback Rust HTTP smoke pass.
- OpenAPI: validated **3.1.0**, checked export drift, and ran oasdiff **1.31.0** against the retained baseline. The unchanged schema passes; a removed-operation negative control fails as expected.
- Generator: OpenAPI Generator **7.23.0** compiles its Rust output but rejects the valid `Draft` fixture. This intentionally failing experiment supports the selected handwritten reqwest/Serde fallback; it is not part of that client's passing suite.
- Cloudflare: the actual isolated Worker passed the Rust smoke workload, then Alchemy destroyed it and its secret binding. A subsequent unauthenticated request returned **404**. No original JPEGs were uploaded. See [API reproduction](../../apps/api/README.md).
- Existing Astro build: **passes**. Root site-only typechecking still reports four unrelated errors in `oxlint.config.ts`, `src/content.config.ts`, and `src/pages/og.png.ts`. Those files and the user's unrelated compiler settings were not changed or staged. The only root TypeScript change excludes the independently checked API package.
- Credentials, databases, photos, raw benchmarks, screenshots, generated SDK output, and build directories are excluded from commits. The ignored test token has mode 0600. No push or parent-issue modification is part of this work.
