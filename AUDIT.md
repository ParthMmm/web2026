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

#### S4. The browser re-asks for every hashed file on every page view (fixed, needs a deploy to confirm)

- **Evidence.** Production: `curl -I https://www.parthm.dev/_astro/inter-latin-wght-normal.Dx4kXJAl.woff2` returns `cache-control: public, max-age=0, must-revalidate`. The stylesheets and scripts return the same. Each page view therefore sends a conditional request for the stylesheet, the 48 kB font and each script before it can use its cached copy.
- **Cause.** That header is Cloudflare's default for static assets. The repo had no `_headers` file.
- **Fix.** `public/_headers` gives `/_astro/*` `Cache-Control: public, max-age=31536000, immutable`. Pages keep the default, so a deploy still shows up at once. Alchemy reads `_headers` from the asset directory (`@alchemy.run/cloudflare/src/Workers/Assets.ts`).
- **Tests.** `tests/site/caching.test.ts`: the built `_headers` marks `/_astro/*` immutable for a year, and every file under `/_astro/` has a content hash in its name. The second test is what makes the first safe.
- **To confirm after the next deploy.** `curl -I https://www.parthm.dev/_astro/<any file>` must show `immutable`. I did not deploy.

#### S5. `/projects/` and `/work/` have no `h1` (fixed)

- **Evidence.** axe-core: `page-has-heading-one` fails on both pages, in both themes and at both widths. Their outline started at `h2` ("Projects"), with each record an `h3`. A screen-reader user who jumps to the page title finds none, and a search engine has no heading for the page.
- **Fix.** `Chapter` takes `heading="h1"`; the two index pages pass it. `Record`, which only those pages use, moves from `h3` to `h2` so the outline stays unbroken. The heading is styled by class, not by tag.
- **No visual change, proven.** The computed-style dump of both pages shows 0 differences at both widths and in both themes.
- **Tests.** `tests/site/headings.test.ts`: every page has exactly one `h1`, and no page skips a heading level.

#### S6. Keyboard focus disappears on the version panels (fixed)

- **Evidence.** On `/projects/versos/`, focus the selected tab and press Tab. `document.activeElement` is `section#panel-v6`, `matches(":focus-visible")` is `true`, and its computed outline is `none`. The panel is a tab stop (`tabindex="0"`) with no focus indicator, on all five project pages. That fails WCAG 2.4.7.
- **Cause.** `Versions.astro` had `[role="tabpanel"]:focus-visible { outline: none; }`, which cancelled the site's own global focus ring.
- **Fix.** Deleted the rule. The panel now gets the same 2px green ring as every other control, on keyboard focus only. A pointer user never sees it.
- **Test.** `tests/site/focus.test.ts`: no rule in any built stylesheet removes the outline from a `:focus` or `:focus-visible` selector.

#### S7. A dead link shows a blank white page (fixed, needs a deploy to confirm)

- **Evidence.** Production: `curl https://www.parthm.dev/nope` returns `404` with a 0-byte body. A visitor who follows a stale link sees an empty browser window, with no way back to the site.
- **Cause.** The site had no `404` page, and the static-asset config left `notFoundHandling` at its default, `"none"`.
- **Fix.** `src/pages/404.astro` renders the normal masthead and footer, an `h1` "Not found", one sentence and a link home. It uses the existing `Chapter` and the same type as the home page lede. `alchemy.run.ts` sets `assets: { notFoundHandling: "404-page" }`. `Layout` takes `noindex`, which also drops the canonical and share tags, because the page has no address of its own.
- **Checked.** axe-core reports 0 violations on the page. The `<head>` of the existing pages is byte-identical before and after the `Layout` change.
- **Tests.** `tests/site/not-found.test.ts`: the build has a `404.html` with an `h1` and a home link inside `main`; it is `noindex` and absent from the sitemap.
- **To confirm after the next deploy.** `curl -i https://www.parthm.dev/nope` must return `404` with the page. The `alchemy.run.ts` line is typechecked but I could not run it: I did not deploy.

#### S8. With site data blocked, the theme is wrong and the toggle is dead (fixed)

- **Evidence.** Chrome, with an init script that makes `window.localStorage` throw `SecurityError` (what Chrome and Safari do when a visitor blocks site data), system theme dark: the page rendered light, `html` had no `dark` class, and clicking the toggle did nothing. The inline theme script's first statement is `localStorage.removeItem("theme")`; it threw, so the script never applied the theme or registered the click handler. This is on every page. `src/lib/figure.ts` already guards `sessionStorage` for the same reason; the theme script did not.
- **Fix.** The script reads, writes and forgets the choice through three guarded functions. With storage blocked, the page follows the system theme and the toggle works for the life of the page.
- **Tests.** `tests/site/theme.test.ts` takes the inline script out of the built home page and runs it in a `node:vm` context that has only what the script touches. "with site data blocked…" failed before the fix. "a stored choice wins over the system theme…" covers the override rule, which had no test.
- **Checked in Chrome** after the fix, storage still blocked: dark theme applied, one click switches to light.
- **State machine.** The theme is two facts: the visitor's choice (`dark`, `light` or none) and the system theme. The resolved theme is the choice, else the system. Toggling to the system's theme clears the choice. That was already explicit enough in three small functions, so I did not restructure it.

### Worker: `now.parthm.dev`

The Worker is deployed and polls three sources every minute, and every 15 seconds while a socket is open. No page on `main` reads it yet (`wip/now-playing-masthead` does). It had no tests, and CI did not typecheck it.

#### W1. A source that fails is silent, and one bad secret stops all three (fixed)

- **Evidence, part 1.** `tests/now-playing/worker.test.ts`, "a malformed Apple Music key does not hide what Last.fm reports": with `APPLE_MUSIC_PRIVATE_KEY` set to a PEM that has a literal `\n` in it (what a one-line `.env` value gives), `refresh()` rejected with `InvalidCharacterError` from `atob`. `poll` awaited the three sources with `Promise.all`, so the Orbis and Last.fm answers were thrown away too. The cron then failed every minute, and an alarm that throws is not rescheduled.
- **Evidence, part 2.** Each reader ended in `catch { return null }` and turned `!response.ok` into `null`. `null` also meant "nothing is playing". A 401 from Apple, which is what an expired Music User Token returns, was therefore the same as silence, and nothing was logged. The README says that token expires about every six months. `observability` is on, but the Worker wrote no log line at all.
- **Cause.** A source's answer was `Track | null`. The type had no way to say "I failed", so failure was either thrown or hidden.
- **Fix.** `sources.ts` has `Answer<Value>`: `Found`, `Nothing` or `Unavailable` with a reason. One function, `attempt`, turns a throw into `Unavailable`. `poll` logs each `Unavailable` as a structured `console.warn` with the source and the reason, then reports the first `Found`. Signing the developer token goes through `attempt` too. The Last.fm reason has the API key removed, because that request carries the key in its URL.
- **Tests.** The test above, plus "an upstream that refuses the request is named in the logs, with the reason". Both failed before. A third, "Orbis is reported ahead of Last.fm when both have a track", pins the order the README promises; it passed before and after.
- **How the tests run.** `tests/now-playing/harness.ts` stubs `cloudflare:workers` with a base class that keeps `ctx` and `env`, gives the object in-memory storage, scripts `fetch` per upstream host, and sets the clock with `setSystemTime`. The object under test is the real `NowPlaying` class. Reads go through `parseNowPlayingMessage`, the parser the masthead will use.
- **Left.** A transient failure of the source that supplied the current track still blanks the line until the next poll. Holding the last track through a failure needs a rule for how long, which is a product choice.

#### W2. The Apple Music lifecycle was three states hidden in one number (fixed)

- **Evidence.** `appleMusic()` kept `{ id, since }` and encoded its state in it: no stored value meant "never read the history", `since: 0` meant "the first song I read, which may be from yesterday, so it never counts as playing", and any other `since` was a real time. The lines `const since = latest === undefined ? 0 : now` and `return since === 0 ? null : toTrack(song)` only make sense once you know that. Nothing tested it, and it could only be run inside a Durable Object with a signed token.
- **Fix.** `workers/now-playing/top-song.ts` names the states (`NeverSeen`, `Undated`, `Dated`) and has one pure function, `observe(known, song, now)`, that returns the next state and whether the song is playing. The stored form is unchanged: `fromStored` and `toStored` map `since: 0` to `Undated` at the storage boundary, so the live object needs no migration.
- **Test.** `tests/now-playing/apple-music.model.test.ts` runs all 1,296 four-poll histories over two songs, two gaps and an Apple Music outage through the real `NowPlaying` class, and compares each answer with a model written without the Worker's bookkeeping. This is a refactor, so the test passed before and after, on purpose. I planted three bugs in the old code (`>` to `>=`, dating the first song, never re-dating) and two in the new code; the test caught all five. It does not catch `since: 0` stored as `since: 1`, because an undated song and a song dated in 1970 behave the same.
- **Not a bug, but worth knowing.** A song paused halfway still shows for its full length, and a song on repeat shows once. Both follow from Apple's API having no play times, and the code comment said so already.
