# Audit, 2026-11-12

Branch `audit/fable-2026-11-12`, started from `2887fbd`. One agent, unattended.

Each finding has an ID, its evidence, a status, and the reason. Status is one of:

- **fixed**: changed on this branch. Each entry names the test that failed before the change, or says why its proof is something else.
- **left**: real, but not changed here. The reason says who decides.
- **rejected**: looked like a problem and is not one.

## Summary

| ID | Finding | Status |
| --- | --- | --- |
| S1 | Four unused font files inlined in every page's stylesheet | fixed |
| S2 | Tailwind made utilities from the site's own class names and prose | fixed |
| S3 | Every internal link costs a 307 redirect | fixed |
| S4 | Hashed assets are re-requested on every page view | fixed, confirm after deploy |
| S5 | `/projects/` and `/work/` have no `h1` | fixed |
| S6 | Keyboard focus is invisible on the version panels | fixed |
| S7 | A dead link shows a blank page | fixed, confirm after deploy |
| S8 | Blocked site data breaks the theme and its toggle | fixed |
| S9 | The Orbis player never finishes (implicit state machine) | fixed |
| S10 | A new page silently borrows the home page's share card | fixed |
| S11 | The two work write-ups were one page typed twice | fixed |
| W1 | A failing now-playing source is silent; one bad secret stops all three | fixed |
| W2 | Apple Music lifecycle: three states in one number (implicit state machine) | fixed |
| C1 | CI passed type errors; `.astro` files were never typechecked | fixed |
| L1 | "Visit" button text contrast is 3.87:1 in the light theme | left: design |
| L2 | `/work/carbon-law-group/` has a title and no body | left: content |
| L3 | `http://www.parthm.dev/` answers 200 over plain HTTP; no HSTS | left: account setting |
| L4 | The shadcn setup and the React integration are unused | left: owner's direction |
| L5 | Alchemy state for both stacks lives only in one machine's `.alchemy/` | left: account and config |
| L6 | `apps/desktop` has a passing check script and no CI | left: cannot verify here |
| L7 | Worker: smaller gaps (CORS, SSE line endings, repeat plays) | left: no consumer yet |
| L8 | Older project versions need JavaScript to read | left: design |
| L9 | Smaller items | left |
| R1 to R6 | Checked and found sound | rejected |

The shared stylesheet went from 24,269 to 5,366 bytes gzip. The root had 0 tests and now has 21, plus a typecheck that covers every source file but one script. Only L1 is still reported by axe.

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

#### S1. Every page downloads four unused font files inside its stylesheet (fixed)

- **Evidence.** `dist/_astro/Layout.*.css` was 71,618 bytes, 24,269 gzip. 18,623 bytes of it were four base64 `url(data:…)` blocks: Geist Mono's Cyrillic Extended subset (as both woff2 and woff), its Vietnamese subset and its symbols subset. No page has a glyph in those ranges. The stylesheet is render-blocking on every page. Production served the same file at 24,124 bytes compressed.
- **Cause.** Vite inlines any asset under 4 kB. Those four files are under 4 kB (2.8 to 3.9 kB), so `unicode-range` could no longer stop the download.
- **Fix.** `astro.config.mjs` sets `vite.build.assetsInlineLimit` to keep `.woff` and `.woff2` as files. Everything else keeps Vite's default.
- **Result.** The shared stylesheet went from 24,269 to 10,109 bytes gzip, 58% smaller. Nothing changes on screen, because the same `@font-face` rules load the same files on demand.
- **Test.** `tests/site/stylesheets.test.ts`, "no stylesheet embeds a font".

#### S2. Tailwind turned the site's own class names and prose into utilities (fixed)

- **Evidence.** In Chrome, `getComputedStyle(document.querySelector(".pairings .table")).display` was `"table"`. The Möte figure has `<span class="table">` with its own scoped rule; Tailwind's scanner read the same word and emitted `.table{display:table}`. The utilities layer of the built stylesheet named 173 classes. Pages use one (`sr-only`). The rest came from words in Markdown and comments (`prose`, `hidden`, `fixed`, `filter`, `shadow`, …), from the typography plugin (12.6 kB of `.prose` rules, used nowhere), and from the shadcn `button.tsx` that no page renders.
- **Cause.** Tailwind scans every source file by default. The site styles itself with scoped CSS and uses Tailwind only for the reset, the theme tokens and `sr-only`.
- **Fix.** `global.css` imports Tailwind with `source(none)` and names `sr-only` in `@source inline()`. Both `@theme` blocks are now `static`, because an unscanned Tailwind drops any token it cannot see in use. My first attempt missed that, and 11 tokens (`--color-ink-1`, `--text-display`, …) vanished from the build; the third test below now guards it.
- **Result.** The shared stylesheet went from 10,109 to 5,366 bytes gzip. With S1 that is 24,269 to 5,366, or 78% smaller.
- **No visual change, proven.** I dumped the computed style of every element and pseudo-element on all 10 pages, at 390 and 1280 px, light and dark, for the old and new builds, and compared them. The only differences are the four `.table` spans: `display` goes from `table` to `block`. They are grid items of a fixed 1.5rem column, so their boxes do not move.
- **Tests.** `tests/site/stylesheets.test.ts`: "Tailwind generates no utility under a class name a component styles itself" and "every custom property a page reads without a fallback is defined". I planted both bugs (the old import; `@theme` without `static`) and each test failed.
- **Trade-off.** A Tailwind utility typed into an `.astro` file now does nothing until it is named in `@source`. The comment at the top of `global.css` says so. Undo by deleting `source(none)`.

#### S3. Every internal link costs a redirect (fixed)

- **Evidence.** In production, `curl -I https://www.parthm.dev/projects/mote` returns `307` with `location: /projects/mote/`. Every link on the site used the slashless form: all 7 write-up links on the home page, every "Projects" and "Work" back link, both cross-links in the write-ups, and the JSON-LD URLs. Five timed pairs put the extra hop at 60 to 180 ms on a fast connection. The two old URLs took three hops: `/projects/tracklister` (307) to `/projects/tracklister/` (meta refresh) to `/projects/versos` (307) to `/projects/versos/`. The canonical URL and the sitemap already used the slash, so the structured data disagreed with them.
- **Fix.** Links, redirect targets and JSON-LD URLs now end in a slash. `trailingSlash: "always"` makes the dev server answer a slashless link with 404, so the mistake shows up before a deploy. The public URLs do not change.
- **Test.** `tests/site/links.test.ts`: every `<a href>`, meta refresh target and absolute site URL in the built pages must be the exact path of a built file. It failed with 37 links.
- **Left.** The two old URLs still redirect with a meta refresh, because Astro's static output has no HTTP redirect. A `public/_redirects` file would make them one real 301. I left it: two files would then describe the same redirect.

#### S4. The browser re-asks for every hashed file on every page view (fixed, needs a deploy to confirm)

- **Evidence.** In production, `curl -I https://www.parthm.dev/_astro/inter-latin-wght-normal.Dx4kXJAl.woff2` returns `cache-control: public, max-age=0, must-revalidate`. The stylesheets and scripts return the same. Each page view therefore sends a conditional request for the stylesheet, the 48 kB font and each script before it can use its cached copy.
- **Cause.** That header is Cloudflare's default for static assets. The repo had no `_headers` file.
- **Fix.** `public/_headers` gives `/_astro/*` `Cache-Control: public, max-age=31536000, immutable`. Pages keep the default, so a deploy still shows up at once. Alchemy reads `_headers` from the asset directory (`@alchemy.run/cloudflare/src/Workers/Assets.ts`).
- **Tests.** `tests/site/caching.test.ts`: the built `_headers` marks `/_astro/*` immutable for a year, and every file under `/_astro/` has a content hash in its name. The second test is what makes the first safe.
- **To confirm after the next deploy.** `curl -I https://www.parthm.dev/_astro/<any file>` must show `immutable`. I did not deploy.

#### S5. `/projects/` and `/work/` have no `h1` (fixed)

- **Evidence.** The axe-core rule `page-has-heading-one` fails on both pages, in both themes and at both widths. Their outline started at `h2` ("Projects"), with each record an `h3`. A screen-reader user who jumps to the page title finds none, and a search engine has no heading for the page.
- **Fix.** `Chapter` takes `heading="h1"`; the two index pages pass it. `Record`, which only those pages use, moves from `h3` to `h2` so the outline stays unbroken. The heading is styled by class, not by tag.
- **No visual change, proven.** The computed-style dump of both pages shows 0 differences at both widths and in both themes.
- **Tests.** `tests/site/headings.test.ts`: every page has exactly one `h1`, and no page skips a heading level.

#### S6. Keyboard focus disappears on the version panels (fixed)

- **Evidence.** On `/projects/versos/`, focus the selected tab and press Tab. `document.activeElement` is `section#panel-v6`, `matches(":focus-visible")` is `true`, and its computed outline is `none`. The panel is a tab stop (`tabindex="0"`) with no focus indicator, on all five project pages. That fails WCAG 2.4.7.
- **Cause.** `Versions.astro` had `[role="tabpanel"]:focus-visible { outline: none; }`, which cancelled the site's own global focus ring.
- **Fix.** Deleted the rule. The panel now gets the same 2px green ring as every other control, on keyboard focus only. A pointer user never sees it.
- **Test.** `tests/site/focus.test.ts`: no rule in any built stylesheet removes the outline from a `:focus` or `:focus-visible` selector.

#### S7. A dead link shows a blank white page (fixed, needs a deploy to confirm)

- **Evidence.** In production, `curl https://www.parthm.dev/nope` returns `404` with a 0-byte body. A visitor who follows a stale link sees an empty browser window, with no way back to the site.
- **Cause.** The site had no `404` page, and the static-asset config left `notFoundHandling` at its default, `"none"`.
- **Fix.** `src/pages/404.astro` renders the normal masthead and footer, an `h1` "Not found", one sentence and a link home. It uses the existing `Chapter` and the same type as the home page lede. `alchemy.run.ts` sets `assets: { notFoundHandling: "404-page" }`. `Layout` takes `noindex`, which also drops the canonical and share tags, because the page has no address of its own.
- **Checked.** axe-core reports 0 violations on the page. The `<head>` of the existing pages is byte-identical before and after the `Layout` change.
- **Tests.** `tests/site/not-found.test.ts`: the build has a `404.html` with an `h1` and a home link inside `main`; it is `noindex` and absent from the sitemap.
- **To confirm after the next deploy.** `curl -i https://www.parthm.dev/nope` must return `404` with the page. The `alchemy.run.ts` line is typechecked but I could not run it: I did not deploy.

#### S8. With site data blocked, the theme is wrong and the toggle is dead (fixed)

- **Evidence.** I ran Chrome with the system theme set to dark and an init script that makes `window.localStorage` throw `SecurityError`, which is what Chrome and Safari do when a visitor blocks site data. The page rendered light, `html` had no `dark` class, and clicking the toggle did nothing. The inline theme script's first statement is `localStorage.removeItem("theme")`; it threw, so the script never applied the theme or registered the click handler. This is on every page. `src/lib/figure.ts` already guards `sessionStorage` for the same reason; the theme script did not.
- **Fix.** The script reads, writes and forgets the choice through three guarded functions. With storage blocked, the page follows the system theme and the toggle works for the life of the page.
- **Tests.** `tests/site/theme.test.ts` takes the inline script out of the built home page and runs it in a `node:vm` context that has only what the script touches. "with site data blocked…" failed before the fix. "a stored choice wins over the system theme…" covers the override rule, which had no test.
- **Checked in Chrome.** After the fix, with storage still blocked, the page is dark and one click switches it to light.
- **State machine.** The theme is two facts: the visitor's choice (`dark`, `light` or none) and the system theme. The resolved theme is the choice, else the system. Toggling to the system's theme clears the choice. Three small functions already say that, so I did not restructure it.

#### S9. The Orbis player never finishes (fixed)

- **Evidence.** I ran the old build in Chrome with an init script that fires the page's one-second interval every millisecond, pressed Play on the Orbis figure, and waited out the 34 minutes left in the set. At `1:59:33 / 1:59:33` the button still read "Pause", the status read "Playing", the level bars still animated, and the interval kept firing. Pause then Play at the end "played" again without moving.
- **Cause.** An implicit state machine. The script kept `playing` (a boolean), `position`, `visible` and a timer handle, and nothing said that the end of the set is a state. `position` was clamped to the length and `playing` stayed `true`.
- **Fix.** `src/components/media/orbis-player.ts` is the player as a tagged union (`Paused`, `Playing`, `Ended`) and one pure function, `step(player, event, length)`, over three events (`Toggle`, `Seek`, `Tick`). It also returns what the status line says, so the copy is in one place. `Ended` has no position; Play from `Ended` starts again at 0:00. The figure's script now only sends events and renders. The copy is unchanged.
- **Tests.** `tests/site/orbis-player.test.ts`: playback stops at the end; Play on a finished set restarts it; and over all 4,096 six-event sequences the player is never playing at or past the end, and only a tick while playing moves it.
- **Checked in Chrome.** On the new build with the same fast clock, at the end the button reads "Play", the bars stop, and the status reads "Saved at 1:59:33 for every device". Pause, resume and cue seeking behave as before.

#### S10. A new page silently borrows the home page's share card (fixed)

- **Evidence.** `ogImagePath("/projects/not-written-yet/")` returned `/og/index.png`. A page's card comes from a hand-kept table in `src/data/og.ts`, separate from `src/data/site.ts` and the Markdown. A page missing from the table shipped with the home page's image and alt text in its link previews, with no warning. Adding a project means editing three or four files, and this one fails quietly.
- **Fix.** A missing card throws, with the path and the fix in the message. I removed one card to check, and the build stopped with: ``No share card for /projects/auriom/. Its link previews would have no image of their own. Fix: add "projects/auriom" to `ogCards` in src/data/og.ts.`` The `Layout` component lost its `image` and `imageAlt` props, which no page passed.
- **Checked.** The `<head>` of five existing pages is identical to the baseline build.
- **Test.** `tests/site/og.test.ts`.
- **Left.** The table itself. The cards for the seven write-ups could be derived from `site.ts` if each record carried its tone, which would remove the table's duplicate titles. That is a data-model change to make together with the registry item in L9.

#### S11. The two work write-ups were the same page, typed twice (fixed)

- **Evidence.** `diff src/pages/work/carbon-law-group.astro src/pages/work/embic.astro` showed four differing lines: the slug, the title, and the description, each as a string literal. Both descriptions were copies of the ones in `src/data/site.ts`, so a page, the home page row and the share card could drift apart. A third write-up would have meant a third copy.
- **Fix.** One route, `src/pages/work/[slug].astro`, builds a page for each entry in the `work` collection and takes the title and description from `site.ts`, as `projects/[slug].astro` already does. Two files deleted, one added: 48 lines to 31.
- **Proof.** `diff -rq` of the whole build before and after reports no difference: every file is byte-identical.
- **Test.** None added. This is a refactor with no behaviour to fail first, and the byte comparison is a stronger check than a test I could write. The link, heading and share-card tests cover the two pages as before.
- **Found on the way, and fixed in its own commit.** `bun test` sets `NODE_ENV=test`, and the build the tests spawn inherited it. Astro then built as if for dev, so the tests were checking a build no visitor gets. The helper now sets `NODE_ENV=production`, and the test build is byte-identical to `astro build`.

### Worker: `now.parthm.dev`

The Worker is deployed and polls three sources every minute, and every 15 seconds while a socket is open. No page on `main` reads it yet (`wip/now-playing-masthead` does). It had no tests, and CI did not typecheck it.

#### W1. A source that fails is silent, and one bad secret stops all three (fixed)

- **Evidence, part 1.** `tests/now-playing/worker.test.ts`, "a malformed Apple Music key does not hide what Last.fm reports": with `APPLE_MUSIC_PRIVATE_KEY` set to a PEM that has a literal `\n` in it (what a one-line `.env` value gives), `refresh()` rejected with `InvalidCharacterError` from `atob`. `poll` awaited the three sources with `Promise.all`, so the Orbis and Last.fm answers were thrown away too. The cron then failed every minute, and `alarm()` never reached the line that schedules the next poll.
- **Evidence, part 2.** Each reader ended in `catch { return null }` and turned `!response.ok` into `null`. `null` also meant "nothing is playing". A 401 from Apple, which is what an expired Music User Token returns, was therefore the same as silence, and nothing was logged. The README says that token expires about every six months. `observability` is on, but the Worker wrote no log line at all.
- **Cause.** A source's answer was `Track | null`. The type had no way to say "I failed", so failure was either thrown or hidden.
- **Fix.** `sources.ts` has `Answer<Value>`: `Found`, `Nothing` or `Unavailable` with a reason. One function, `attempt`, turns a throw into `Unavailable`. `poll` logs each `Unavailable` as a structured `console.warn` with the source and the reason, then reports the first `Found`. Signing the developer token goes through `attempt` too. The Last.fm reason has the API key removed, because that request carries the key in its URL.
- **Tests.** The test above, plus "an upstream that refuses the request is named in the logs, with the reason". Both failed before. A third, "Orbis is reported ahead of Last.fm when both have a track", pins the order the README promises; it passed before and after.
- **How the tests run.** `tests/now-playing/harness.ts` stubs `cloudflare:workers` with a base class that keeps `ctx` and `env`, gives the object in-memory storage, scripts `fetch` per upstream host, and sets the clock with `setSystemTime`. The object under test is the real `NowPlaying` class. Reads go through `parseNowPlayingMessage`, the parser the masthead will use.
- **Checked in workerd.** I bundled the Worker and ran it in Miniflare 4 with a fake Last.fm and a malformed Apple key. After the cron, `GET /` returned the Last.fm track; with Last.fm answering 401 it returned `{"playing":null}`, and workerd printed one warning per failed source with its reason.
- **Cost.** A source that stays down now writes one log line per poll: 1,440 a day from the cron, more while a socket is open. Those lines are how the owner learns that a token expired. The volume is far below the Workers Logs daily allowance, but an Orbis server that is off on purpose writes them too.
- **Left.** A transient failure of the source that supplied the current track still blanks the line until the next poll. Holding the last track through a failure needs a rule for how long, which is a product choice.

#### W2. The Apple Music lifecycle was three states hidden in one number (fixed)

- **Evidence.** `appleMusic()` kept `{ id, since }` and encoded its state in it: no stored value meant "never read the history", `since: 0` meant "the first song I read, which may be from yesterday, so it never counts as playing", and any other `since` was a real time. The lines `const since = latest === undefined ? 0 : now` and `return since === 0 ? null : toTrack(song)` only make sense once you know that. Nothing tested it, and it could only be run inside a Durable Object with a signed token.
- **Fix.** `workers/now-playing/top-song.ts` names the states (`NeverSeen`, `Undated`, `Dated`) and has one pure function, `observe(known, song, now)`, that returns the next state and whether the song is playing. The stored form is unchanged: `fromStored` and `toStored` map `since: 0` to `Undated` at the storage boundary, so the live object needs no migration.
- **Test.** `tests/now-playing/apple-music.model.test.ts` runs all 1,296 four-poll histories over two songs, two gaps and an Apple Music outage through the real `NowPlaying` class, and compares each answer with a model written without the Worker's bookkeeping. This is a refactor, so the test passed before and after, on purpose. I planted three bugs in the old code (`>` to `>=`, dating the first song, never re-dating) and two in the new code; the test caught all five. It does not catch `since: 0` stored as `since: 1`, because an undated song and a song dated in 1970 behave the same.
- **Not a bug, but worth knowing.** A song paused halfway still shows for its full length, and a song on repeat shows once. Both follow from Apple's API having no play times, and the code comment said so already.

### Checks: what stops a bad change

#### C1. CI passed code that does not typecheck, and nothing checked `.astro` files (fixed)

- **Evidence, part 1.** At `2887fbd` I planted `const LIVE_POLL_MS: string = 15_000` in the Worker and `const isHome: number = …` in `Masthead.astro`, then ran CI's only command for the site. `bun run check` exited 0. CI ran lint and format, never the typechecker, a test or a build.
- **Evidence, part 2.** `bun run typecheck` failed on a fresh clone (`Cannot find module 'astro:content'`): it needs `.astro/types.d.ts`, which only a build or `astro sync` writes. It also skipped `workers/`, and `tsc` cannot read `.astro` files at all. Those 22 files hold every page and component: about 350 lines of front matter and 410 lines of client TypeScript in `<script>` blocks.
- **Evidence, part 3.** `astro check`, run for the first time, reported 2 errors: `[slug].astro` passes `string | undefined` to `Versions` (`period`) and to `Article` (`url`, `urlLabel`), which `exactOptionalPropertyTypes` rejects. Neither changes behaviour.
- **Fix.** `bun run typecheck` now runs `astro sync`, then `tsc` for the site, the tests and the Worker, then `astro check`. CI runs `typecheck` and `test` after `check`. The two prop types accept `undefined`. `@astrojs/check` 0.9.12 needs TypeScript 5 or 6 and the repo is on 7, so `tools/astro-check/` pins the checker and TypeScript 5.9.3 in its own lockfile.
- **Proof.** With the fix, each planted error fails `bun run typecheck`. The whole command takes 4.8 s from a clean checkout.
- **Test.** The check is the test here: `bun run typecheck` failed before (2 `.astro` errors, plus the fresh-clone failure) and passes after. A test that asserts on the workflow file would only detect edits to it.
- **Left.** The pre-commit hook still runs lint and format only. `typecheck` and `test` add about 8 s to each commit; rat-stack does run them there. Owner's call.
- **Left.** `workers/now-playing/authorize.ts` is a Bun script outside every `tsconfig`. It is 71 lines and runs once every six months by hand.

## Left for the owner

#### L1. The "Visit" button fails WCAG contrast in the light theme

- **Evidence.** axe-core, `color-contrast`, serious, on `/projects/versos/`, `/orbis/`, `/mote/` and `/auriom/` in the light theme. The label is cream-50 on green-600 at 15px, weight 500: 3.87:1 by the WCAG 2 formula, against a 4.5:1 minimum. The dark theme passes (7.92:1).
- **Why left.** The comment in `global.css` records the choice by APCA ("primary label on primary 68"), so it is deliberate, and the fix changes the brand fill on the one filled control. Green-700 as the light-theme fill gives 5.72:1.

#### L2. `/work/carbon-law-group/` has no body

- **Evidence.** `src/content/work/carbon-law-group.md` is front matter only. The built page is the title, the one-line description, and an empty `<div class="content">`. The home page links to it as a write-up.
- **Why left.** Only the owner can write it. Until then, dropping `url` from the record in `site.ts` turns the row into plain text, which is what the three landing-page rows already do.

#### L3. The site answers over plain HTTP

- **Evidence.** `curl -I http://www.parthm.dev/` returns `200 OK`, not a redirect. `https://www.parthm.dev/` sends no `Strict-Transport-Security` header.
- **Why left.** "Always Use HTTPS" and HSTS are Cloudflare zone settings on the owner's account. HSTS is also hard to undo once browsers have cached it.

#### L4. The shadcn setup is unused

- **Evidence.** No page imports `src/components/ui/button.tsx`, `src/lib/utils.ts` or `src/icons/BookIcon.astro`, on `main` or on any of the four local feature branches I checked. No page has a `client:` directive, so the React integration renders nothing, yet every build uploads a 220 kB `client.*.js` that no page references. `@base-ui/react`, `class-variance-authority`, `clsx`, `cn`, `lucide-react`, `shadcn`, `tailwind-merge`, `tw-animate-css` and `@tailwindcss/typography` are installed for it.
- **Why left.** The redesign commit added it on purpose, a comment in `global.css` says a redesign will replace the current tokens, and the lint config protects `src/components/ui` for `shadcn add`. After S2 none of it reaches a visitor. Removing it is a decision about where the design system is going. If it goes: delete those three files, `components.json`, the `react()` integration, and the packages above; keep `react` for the share-card renderer.

#### L5. Deploy state exists on one machine

- **Evidence.** Both stacks use `Alchemy.localState()`, and `.alchemy/` is git-ignored. The README says not to delete it while the Worker is deployed, because it tracks ownership. This worktree has no `.alchemy/`, so a deploy from here would not know the live resources.
- **Why left.** Moving state to a remote store needs a Cloudflare resource and a deploy. It is the first thing I would do with the owner present: a lost laptop today means adopting every resource by hand.

#### L6. The desktop app is not in CI

- **Evidence.** `.github/workflows/` has a job for the site and one for `apps/api`, none for `apps/desktop`. Its gate, `apps/desktop/scripts/check`, passes here: format, shape, clippy and test all `ok`.
- **Why left.** The job needs a macOS runner, Xcode's Metal toolchain and a pinned libvips 8.18.3 that Homebrew may no longer serve. I could not prove such a workflow without watching CI, which this run does not do, and a red job on every push is worse than none. The repository is public, so the runner minutes are free.

#### L7. Worker: smaller gaps

- `GET /` on `now.parthm.dev` sends no `Access-Control-Allow-Origin`, so a page could not `fetch()` it. The planned masthead uses the WebSocket, which needs none.
- The Orbis reader splits server-sent events on `\n\n` only. A server that sends `\r\n` would never match; since W1 that shows in the logs as "no presence frame within 5000 ms".
- `ORBIS_API_URL` in `alchemy.run.ts` is a tailnet hostname in a public repository. Funnel hostnames are public in certificate logs anyway.
- A transient failure blanks the line for one poll, and a repeated or paused song is misreported (see W1 and W2).

#### L8. Older versions of a project need JavaScript

- **Evidence.** `Versions.astro` renders every panel but the latest with `hidden`, and only its script can change that. With JavaScript off, v1 to v5 of Versos cannot be read.
- **Why left.** The fix is a layout choice (stack the versions, or use `<details>`), not a repair.

#### L9. Smaller items

- The two retired project URLs redirect with a meta refresh, not a 301 (see S3).
- The title, URL and site of each flagship are typed in both `index.astro` and `site.ts`, and again in the share-card table (see S10). One registry would do.
- `package.json` is still named `web2024`.
- Chrome treats `apple-mobile-web-app-capable` as deprecated in favour of `mobile-web-app-capable`. iOS still needs the Apple one.
- The pre-commit hook runs lint and format only (see C1).

## Checked and found sound

- **R1. Owner-token check in `apps/api`.** It hashes both tokens and compares the digests with `Mac.timingSafeEqual`, and config rejects a token under 32 characters. `apps/api` as a whole: typecheck, 50 Node tests, 13 workerd tests and the OpenAPI check all pass, and its lifecycle (`Uploading`, `Draft`; `Staged`, `Final`, `Missing`) is already tagged unions.
- **R2. Small tap targets.** My script flagged links under 24px tall. They are links inside sentences, which WCAG 2.5.8 exempts, or the footer links, which have the spacing it asks for.
- **R3. Map labels in the Beacon figure.** axe marks their contrast "needs review". They sit inside an SVG with `role="img"` and a label, so they are not exposed as text.
- **R4. Font preload.** I served the fonts 1.5 s late. Cumulative layout shift stayed at 0.01 or below at both widths, so a preload would buy nothing measurable here. The fallback on Windows and Android differs, so this holds for Apple devices only.
- **R5. The figures' step sequences.** `sequence()` in `figure.ts` gives each run an id and a stale run stops at its next wait. The steps are numbered states in `data-step`. They are explicit already.
- **R6. The 55% ink (`--color-ink-4`).** 3.73:1 on cream. The only text it colours is the display-size half of the home page heading, where 3:1 is the minimum. Its other uses are decoration: list markers, a dash, an icon, and the map labels of R3.

## What I would do next

1. Deploy, then run the two `curl` checks under S4 and S7.
2. Move Alchemy state off the laptop (L5).
3. Decide L1 and L4 together: they are both about the design system.
4. Turn the masthead line on from `wip/now-playing-masthead`. The Worker now has tests and failure logs. Decide the hold-through-failure rule (W1) first.
5. Run axe in CI. The audit's accessibility findings came from a browser by hand; the tests catch the two classes I fixed, not new ones.
