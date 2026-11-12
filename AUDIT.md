# Audit, 2026-11-12

Branch `audit/fable-2026-11-12`, started from `2887fbd`. One agent, unattended.

Each finding has an ID, its evidence, a status, and the reason. Status is one of:

- **fixed**: changed on this branch, with a test that failed before the change.
- **left**: real, but not changed here. The reason says who decides.
- **rejected**: looked like a problem and is not one.

## How I measured

- Build: `bun run build`, then read `dist/`.
- Browser: `agent-browser` (Chrome over CDP) against `astro preview`, at 320, 390, 768 and 1280 px wide, in the light and dark themes. axe-core 4.12.1 ran on all ten pages in each combination.
- Production, read only: `curl` against `https://www.parthm.dev` and `https://now.parthm.dev`. Nothing was deployed or changed.
- Standards: the repo `AGENTS.md`, `apps/desktop/AGENTS.md`, and rat-stack (`ratstack.sh/AGENTS.md`, `skills/test-behaviour`, `skills/add-a-lifecycle-machine`).

## Baseline at `2887fbd`

| Check | Result |
| --- | --- |
| `bun run check` (oxlint + oxfmt) | passes |
| `bun run typecheck` on a fresh clone | **fails**: `Cannot find module 'astro:content'` until a build writes `.astro/types.d.ts` |
| `bun run build` | passes, 10 pages, 1.7 s |
| Root tests | **none**. No test runner, no test file, for the site or the Worker |
| CI for the site and Worker | lint and format only. No typecheck, test, or build |
| axe violations | 2 pages with no `h1`; 4 pages with a contrast failure (light theme) |
| Horizontal overflow, 320 to 1280 px | none |
| Shared stylesheet, gzip | 24,269 bytes local; 24,124 bytes from production |
| Local vitals for `/` | FCP 80 ms, LCP 96 ms (the `h1`), CLS 0 |

## Decisions I made without the owner

- **No XState.** rat-stack models lifecycles as XState machines. This repo has no XState, and rat-stack also says to record drift, not migrate silently. Where I made a state machine explicit, I used a tagged union and a pure transition function. That adds no dependency and is one file to replace if the repo adopts XState.
- **`bun test` for the root.** `apps/api` uses Vitest. The root had no runner. Bun is already the package manager and script runtime, so `bun test` adds one dev dependency (`@types/bun`, already pinned in `apps/api`).
- **Tests read the build output.** The site is static, so most site tests build once and assert on `dist/`. That checks what visitors get, and needs no browser in CI.

## Findings

### Site: what visitors get

#### S1. Every page downloads four unused fonts inside its stylesheet (fixed)

- **Evidence.** `dist/_astro/Layout.*.css` was 71,618 bytes, 24,269 gzip. 18,623 bytes of it were four `url(data:font/woff2;base64,…)` blocks: the Geist Mono subsets for Cyrillic Extended, Cyrillic, Vietnamese and symbols. No page has a glyph in those ranges. The stylesheet is render-blocking on every page. Production served the same file at 24,124 bytes compressed.
- **Cause.** Vite inlines any asset under 4 kB. Those four subset files are under 4 kB, so `unicode-range` could no longer stop the download.
- **Fix.** `astro.config.mjs` sets `vite.build.assetsInlineLimit` to keep `.woff` and `.woff2` as files. Everything else keeps Vite's default.
- **Result.** Shared stylesheet: 24,269 to 10,109 bytes gzip (58% smaller). No visual change: the same `@font-face` rules load the same files on demand.
- **Test.** `tests/site/stylesheets.test.ts`, "no stylesheet embeds a font".
