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

#### S2. Tailwind turned the site's own class names and prose into utilities (fixed)

- **Evidence.** In Chrome, `getComputedStyle(document.querySelector(".pairings .table")).display` was `"table"`. The Möte figure has `<span class="table">` with its own scoped rule; Tailwind's scanner read the same word and emitted `.table{display:table}`. The built stylesheet held 173 utility classes. Pages use one (`sr-only`). The rest came from words in Markdown and comments (`prose`, `hidden`, `fixed`, `filter`, `shadow`, …), from the typography plugin (12.6 kB of `.prose` rules, used nowhere), and from the shadcn `button.tsx` that no page renders.
- **Cause.** Tailwind scans every source file by default. The site styles itself with scoped CSS and uses Tailwind only for the reset, the theme tokens and `sr-only`.
- **Fix.** `global.css` imports Tailwind with `source(none)` and names `sr-only` in `@source inline()`. Both `@theme` blocks are now `static`, because an unscanned Tailwind drops any token it cannot see in use. My first attempt missed that, and 11 tokens (`--color-ink-1`, `--text-display`, …) vanished from the build; the third test below now guards it.
- **Result.** Shared stylesheet: 10,109 to 5,366 bytes gzip. With S1, 24,269 to 5,366 (78% smaller).
- **No visual change, proven.** I dumped the computed style of every element and pseudo-element on all 10 pages, at 390 and 1280 px, light and dark, for the old and new builds, and compared them. The only differences are the four `.table` spans: `display` goes from `table` to `block`. They are grid items of a fixed 1.5rem column, so their boxes do not move.
- **Tests.** `tests/site/stylesheets.test.ts`: "Tailwind generates no utility under a class name a component styles itself" and "every custom property a page reads without a fallback is defined". I planted both bugs (the old import; `@theme` without `static`) and each test failed.
- **Trade-off.** A Tailwind utility typed into an `.astro` file now does nothing until it is named in `@source`. The comment at the top of `global.css` says so. Undo by deleting `source(none)`.

#### S3. Every internal link costs a redirect (fixed)

- **Evidence.** Production: `curl -I https://www.parthm.dev/projects/mote` returns `307` with `location: /projects/mote/`. Every link on the site used the slashless form: all 7 write-up links on the home page, every "Projects" and "Work" back link, both cross-links in the write-ups, and the JSON-LD URLs. Five timed pairs put the extra hop at 60 to 180 ms on a fast connection. The two old URLs took three hops: `/projects/tracklister` (307) to `/projects/tracklister/` (meta refresh) to `/projects/versos` (307) to `/projects/versos/`. The canonical URL and the sitemap already used the slash, so the structured data disagreed with them.
- **Fix.** Links, redirect targets and JSON-LD URLs now end in a slash. `trailingSlash: "always"` makes the dev server answer a slashless link with 404, so the mistake shows up before a deploy. The public URLs do not change.
- **Test.** `tests/site/links.test.ts`: every `<a href>`, meta refresh target and absolute site URL in the built pages must be the exact path of a built file. It failed with 37 links.
- **Left.** The two old URLs still redirect with a meta refresh, because Astro's static output has no HTTP redirect. A `public/_redirects` file would make them one real 301. I left it: two files would then describe the same redirect.
