# Photo prototype

A macOS photo library built with [Rune](https://github.com/anomalyco/rune) and rune-kit. It implements the persistent local library from [issue #5](https://github.com/ParthMmm/web2026/issues/5) on top of the performance gate from [issue #3](https://github.com/ParthMmm/web2026/issues/3). It uploads photos to the photo API in `apps/api` as private drafts, and publishes them to the public gallery and takes them back out (see [Upload](#upload) and [Publish](#publish)). Photo editing is not built yet.

## Run on macOS

Requires Xcode with its Metal toolchain, Rust, and **libvips 8.18.3**. The app checks `vips --version` before any Vips import or preview, rejects other versions, and caches a successful check for the process. Rune and rune-kit are pre-release and pinned to `dev` revision `3e0cf8fe58d592cdda1ec5533461192edcfcdb47`. `Cargo.lock` started from that revision's own lock file.

If Homebrew still provides 8.18.3, install it with `brew install vips`, then verify and pin it:

```sh
test "$(vips --version)" = "vips-8.18.3" && brew pin vips
```

If Homebrew has moved on, extract the archived version into a local tap instead. This avoids upgrading/downgrading the unversioned helper used by other projects:

```sh
brew tap --force homebrew/core
brew tap-new local/photo-prototype
brew extract --version=8.18.3 vips local/photo-prototype
brew install local/photo-prototype/vips@8.18.3
export PATH="$(brew --prefix local/photo-prototype/vips@8.18.3)/bin:$PATH"
test "$(vips --version)" = "vips-8.18.3"
```

The recorded checks used an existing 8.18.3 install; archived-formula installation was not rerun. Bundling and a fully pinned native build toolchain remain distribution work.

```sh
cargo run --release --locked --manifest-path apps/desktop/Cargo.toml --features desktop --bin photo-desktop -- "$HOME/Pictures"
```

Arguments are JPEG files or folders to import on launch. Rune reads its own flags first (`--help` lists them) and passes the rest to the app. You can also launch without arguments and use **Import…** (Cmd-O) or **Import from Photos…** (Cmd-Shift-O), from the title bar or the File menu. Imports queue, so you can start another one while one is running. The status bar shows progress and a one-line summary when the import finishes.

- **Browse:** click a photo to select it, or use the arrow keys, Page Up/Down, and Home/End (or Cmd-Up/Down). The inspector shows the large preview, dimensions, file size, camera, lens, exposure, capture time, and the original's location.
- **Arrange:** the slider in the title bar, or Cmd-+ and Cmd--, sets the photo size. Ctrl-Cmd-S shows or hides the sidebar, and Cmd-I shows or hides the inspector. Right-click a photo for Open, Show Info, Show in Finder (also Cmd-Alt-R), Publish, and Unpublish.
- **View:** double-click, Space, or Return opens the full-window viewer. Left/Right step through photos; Escape or Space closes it and returns focus to the grid.
- **Failures:** files that couldn't be imported appear under **Needs attention › Not imported** in the sidebar, with their count (also File › Files Not Imported…). The sheet shows the reason for each file, and offers **Retry all** and **Clear list**.
- **Performance:** the chip button in the title bar (Cmd-Alt-P, or View › Show or Hide Performance) shows the frames drawn in the last second and the slowest frame's CPU time, from Rune's per-frame reports. An idle window draws no frames, so the panel then says it is idle.

The theme follows the system's light or dark appearance. Both palettes live in `src/ui/look.rs`, which lays them over rune-kit's tokens.

On macOS 13 and later, **Import from Photos…** opens the native Photos picker. The picker reads only the assets you select; it does not open the Photos library package or database. JPEG representations are copied into the library's `imports/` folder, and other image representations are converted to JPEG with the system `sips` tool. Those copies become the photos' originals. A copy that duplicates a photo already in the library is deleted. A copy that fails to import stays in the failure list, so **Retry all** can read it; **Clear list** deletes it. Set `PHOTO_IMPORT_DIR` to store copies elsewhere.

Selecting a folder imports JPEGs from all nested subfolders. Photos library packages and Photo Booth are excluded, and symlinks are skipped. Folder names do not become albums.

## The library

The library lives in `~/Library/Application Support/dev.parth.photo-prototype/library`; set `PHOTO_LIBRARY_DIR` to use another one. It contains a SQLite catalog (`catalog.sqlite`) and generated `derivatives/`. Originals are only read, never modified or moved.

- **Identity and duplicates.** A photo's identity is the BLAKE3 hash of its bytes. Importing the same bytes from another path adds a known location instead of a second photo. The inspector shows "Found in N places". Re-importing a known, unchanged path (same size and modification time) skips hashing entirely.
- **Missing and moved originals.** On open, sources that no longer exist are flagged. The grid marks a photo whose every known original is missing, and the inspector explains how to reconnect it: import the file from its new location. Existing previews stay available. If a file's content changes in place, it becomes a new photo. The old photo is removed once none of its sources still hold its bytes, unless it is in the cloud.
- **Failures.** Each failed file is recorded with a readable reason, for example "Not a readable JPEG" or "The JPEG is incomplete or damaged". A later successful import clears the record. Failures persist across launches until retried or cleared.
- **Derivatives.** libvips produces WebP renditions at 480 px (grid), 1200 px (mobile), and 2400 px (desktop) on the long edge, never upscaled. Renditions apply EXIF orientation, are converted to sRGB from the embedded ICC profile, and carry no metadata: no EXIF, GPS, ICC, or XMP chunks. The grid rendition is made during import. The larger ones are prepared in the background and on demand for the viewer. Recipe version 1 uses quality 82, which is provisional. A changed recipe version or a missing file is regenerated.
- **Metadata.** ISO, exposure time, aperture, focal length, camera, lens, and capture time (with its UTC offset, when recorded) are read locally from EXIF and stored in the catalog. GPS and serial numbers are ignored. Malformed metadata never fails an import.

## Upload

The app uploads photos to the photo API (`apps/api`) as private drafts. Only the owner can see a draft. [Publish](#publish) makes a draft public.

`PHOTO_API_URL` sets the API's address. `PHOTO_API_TOKEN` sets the owner token. Without it, the app reads the Keychain item with service `dev.parth.photo-prototype` and account `owner-token`, which you can add with this command (it asks for the token):

```sh
security add-generic-password -s dev.parth.photo-prototype -a owner-token -w
```

With no address or token, the inspector says the app is not connected, and the app stays a local library. The token never reaches the catalog, a log, or an error message.

- **Selection.** A click selects one photo. ⌘-click adds a photo to the selection or removes it, Shift-click selects the run of photos from the last photo clicked, and Shift with an arrow key grows or shrinks that run. ⌘A selects every photo the grid shows, and a plain arrow key goes back to one photo. The viewer and the description follow the photo clicked or moved to last. With several photos selected, the inspector shows how many, and the status bar says "24 photos · 3 selected".
- **Actions.** **File ▸ Upload Photo** (⌘U), the inspector's **Upload** button, or **Upload** in a tile's context menu uploads each selected photo that is only in the library, and skips the rest. With several photos selected, the File menu says **Upload Photos**, **Publish Photos** and **Unpublish Photos**, and the inspector lists each step some of them can take, with how many it sends, for example "2 photos to upload". A command is on when at least one selected photo can take it. A right-click on a photo outside the selection selects that photo first; inside it, the menu acts on the whole selection. **Upload All Photos** (⇧⌘U) uploads every photo that is only in the library and retries each failure that a retry can fix. **Cloud Problems…**, or the sidebar's **Cloud problems** row, lists each upload, publish, or unpublish that stopped, with the step and its reason, and **Retry all** retries each one with the step it needs.
- **States.** The catalog keeps one cloud state per photo: Local, Queued, Uploading (with the files the server still lacks), Draft, Publishing, Published, Unpublishing, or Failed with a reason and the step that stopped. A tile in the cloud carries a mark with its own icon and an accessible label, so no state depends on colour. The status bar counts the run, then sums it up, for example "Uploaded 16 photos as private drafts · 1 failed".
- **Steps.** A photo's cloud identity is the SHA-256 of its original. The app computes it in the same pass as the BLAKE3 identity, with the same changed-file checks. To prepare a photo, the app copies its three WebP renditions to `uploads/<photo>/`, so the files the manifest describes cannot change, and records the manifest. A transfer lane then calls `begin`, streams each file the server reports missing from disk with its length, and calls `finalize`. No file is read whole into memory. Each call is safe to repeat. The upload ends in the state the server reports: a draft, or Published when another run already published the photo. The staged copies are deleted when the upload ends.
- **Recovery.** The app records each state before the network call that depends on it. After a restart, it prepares queued photos again and resumes uploading photos from the server's answer to `begin`. So a lost response never sends a second upload or loses a finished one. Timeouts, lost connections, and 5xx answers retry after 2 s, doubling up to 60 s, five attempts in all. Then the photo fails as unavailable, and a retry starts again. A 401 tells you to check the token. A manifest conflict or a rejected finalize stops that photo, because a retry would send the same files. If a file can't be read, or the server refuses its bytes, the app checks the file again: one that went missing or changed fails with that reason, which a person can act on. A retry always sends whole files. The client itself sends a request once more when the connection drops before an answer.
- **Changed originals.** If an original goes missing or changes before its bytes are sent, the upload fails and says what to do. If a photo in the cloud has its original changed in place, the photo stays in the library, detached from that path, so its cloud record is kept.

Two transfer lanes do the network work for uploads, publishes, and unpublishes, apart from the two image lanes, so a call never holds an encoder or delays a preview.

Upload tests (`tests/upload.rs`) run the library against an in-process fake API (`tests/support/mod.rs`) that follows the Worker's contract and injects faults. They cover:

- the whole path to a draft, and the state after a reopen;
- a restart mid-upload, which sends the original only once;
- a lost answer to a PUT and to `finalize`;
- an upload whose `begin` or `finalize` reports a published photo, which stays published;
- a prepared copy that goes missing before it is sent;
- 503 answers that recover, and 503 answers that reach the cap and then a retry;
- a 401, and a rejected `finalize`;
- a photo queued twice;
- an original that changes or goes missing before the upload, and one that changes after it is prepared;
- previews served while uploads run;
- a photo in the cloud whose original changes in place;
- a library with no API set up.

`upload-check` imports JPEGs into a new library, uploads them through the library, asks the API with its own client whether each photo is a draft of its original, then reopens the catalog to confirm the drafts persisted. It then checks publish and unpublish against the public gallery (see [Publish](#publish)). Run it only against a local Worker:

```sh
cd apps/api && bun install --frozen-lockfile && OWNER_TOKEN=<32 or more characters> PORT=8791 bun scripts/serve.ts
# In another shell, from the repository root:
PHOTO_API_URL=http://127.0.0.1:8791 PHOTO_API_TOKEN=<the same token> \
  cargo run --locked --manifest-path apps/desktop/Cargo.toml --bin upload-check -- /tmp/upload-check-library /path/to/jpegs
```

Schema v4 adds the `uploads` table and a photo's detached source.

## Publish

**File ▸ Publish Photo** (⌘P), the inspector's **Publish** button, or **Publish** in a tile's context menu puts each selected draft in the public gallery. **Unpublish Photo** (⇧⌘P), or **Unpublish** in the same places, takes each selected published photo back out; it is a private draft again. Each skips the selected photos it can't act on: Publish acts only on drafts, and Unpublish only on published photos.

- **What is sent.** The capture time and film simulation the catalog read from EXIF, and the photo's description as alt text. Nothing else from EXIF leaves the library.
- **Description.** The inspector edits it under the cloud state, with a count against the API's 300-character limit. **Save**, ⌘↩, or selecting another photo keeps it in the catalog as one line. When the photo is published, the app publishes it again with the new words.
- **States.** The catalog says Publishing or Unpublishing before the call. A tile mark shows a globe for Published, a cloud for Draft, and a clock while either call runs, each with an accessible label. The inspector says who can see the photo: "Only you can see it" for a draft, "Anyone can see it in the gallery" when published, and "It may still be public" after an unpublish that stopped. The status bar says "Publishing 2 photos…" while calls run, then "Published DSCF0001.JPG" or why it couldn't.
- **Recovery.** Publish and unpublish use the upload lanes' retry: timeouts, lost connections, and 5xx answers retry with the same backoff, then fail as unavailable with a retry. Each call is safe to repeat, so a lost answer ends in the right state once. An unpublish that the server stops midway (503) stays Unpublishing until a retry finishes it; the app never shows Draft while the server still says Unpublishing. A publish of a photo the server holds as an unfinished upload fails, and its retry finishes the upload first. A photo the server doesn't know fails as not in the cloud, and a retry uploads it again.
- **Restart and reconcile.** A restart resumes each Publishing or Unpublishing photo and takes the server's answer. When the library opens with an API, a transfer lane reads the server's photo list once (at most 50 pages of 200) before any other call, and corrects local states that disagree: a draft the server published, a publish whose answer was lost, or a photo the server no longer has, which becomes Local. A photo past the pages read is left as it is.

Publish tests (`tests/publish.rs`) use the same fake API. They cover publish and unpublish with the details sent, lost answers to both, an unpublish that stops midway once and every time, a restart during each, a publish refused because the upload is unfinished, a refused token, the check on open, a changed description on a published photo, and an API that is unavailable for a while.

Window press tests (`src/ui/tests/cloud.rs`) open the real view on a library connected to the same fake API, so each press runs its call and the answer lands before the test reads the window. They press Publish, Unpublish, Retry after a publish that stopped, the description field with Save on a published photo, Publish in a tile's context menu, Upload on a selection of two local photos and a published one, Publish on two drafts at once, and the **Drafts** row after a publish.

`upload-check` also publishes every photo, reads `<api>/gallery/v1.json` with no token and finds each one there (one with its description as alt text), then unpublishes them and finds none.

Schema v5 adds a photo's description.

## Processing and memory bounds

The grid virtualizes rows. Columns follow the window width (square tiles, at least the zoom's width, 6 px apart), and the inspector collapses below 760 px. Encoded renditions stay on disk. Decoded images go through `rune::ImageCache`, held in two bounded slot lists (`src/ui/images.rs`): at most **160 thumbnail / 2 viewer entries** interactively (**64 / 2** in benchmark mode), with at most **4 / 1 in-flight decodes**. Only visible rows, plus one row each side, request thumbnails. A pending decode is never evicted, including one for a row that scrolled off-screen. Available thumbnails stay visible until the larger preview arrives. Image work stays off the UI thread.

Two worker lanes allow at most two encoder processes. The background lane runs, in priority order: clearing failures, queuing and preparing uploads, scanning, importing, grid repairs, then larger web sizes. It waits while an interactive preview is pending or running. The interactive lane renders the selected photo's desktop rendition. The latest pending preview replaces older ones. A running background job is never cancelled midway through a write.

Library tests (`tests/library.rs`) cover:

- persistence across restarts, and originals left unchanged;
- nested and overlapping imports;
- content duplicates;
- damaged files, which are remembered, retried, and clearable;
- EXIF orientation;
- Display P3, untagged, and invalid ICC profiles;
- metadata extraction, including that the serial number never reaches disk and that derivatives are stripped of metadata;
- malformed metadata;
- moved originals and in-place content replacement;
- preview priority behind imports, and repeated-preview cache hits;
- web-size preparation;
- deletion of app-owned duplicate copies;
- rejection of other libvips versions.

## Verify

```sh
cargo fmt --manifest-path apps/desktop/Cargo.toml --all -- --check
cargo test --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo check --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo build --release --locked --manifest-path apps/desktop/Cargo.toml --features desktop --bins
python3 apps/desktop/benchmark.py "$HOME/Pictures" --mode pipeline --count 20 --output apps/desktop/benchmarks/local/pipeline-run
caffeinate -dimsu python3 apps/desktop/benchmark.py "$HOME/Pictures" --count 200 --seconds 30 --output apps/desktop/benchmarks/local/gui-run
```

Repeat the GUI command with counts 20, 200, and the full authorized library. Then run it once more with `--uploads` while a local Worker runs (see [Upload](#upload)):

```sh
PHOTO_API_URL=http://127.0.0.1:8791 PHOTO_API_TOKEN=<the Worker's token> \
  caffeinate -dimsu python3 apps/desktop/benchmark.py "$HOME/Pictures" --count 200 --seconds 30 --uploads --output apps/desktop/benchmarks/local/gui-uploads-run
```

`--uploads` sets `PHOTO_BENCH_UPLOADS=1`. The app then uploads each photo as a private draft as soon as it lands, so the transfer lanes run while the window scrolls and loads previews. An upload run measures the cold state only: after it, the server holds every selected photo, so a warm run would send nothing. Start each upload run against a Worker that holds none of the selected photos. The report's `uploads` key counts the photos that are local, pending, in the cloud, and failed when the run ends, and `transferred`: the uploads that sent their files and settled during the run. A photo the server already holds does not count. An upload run passes only when it also meets every budget, transfers at least one photo, and stops none. Without `--uploads`, the benchmark never uploads.

Output directories must be new. Each run creates its own library inside the output directory, so benchmarks never touch your real library. The harness samples evenly across the folder tree and omits source names and paths from JSON. Libraries and raw reports are ignored by Git. Encoder stderr and scene captures can contain private material; keep them local.

`benchmark-pipeline <library-directory> <JPEG-or-folder>...` imports without a window and prints one JSON summary. `PHOTO_PREWARM_INDICES` also renders the desktop previews for those grid positions, which the warm GUI run requires.

### Conditions and measurement

- **Cold:** empty library, not a flushed OS file cache. Repeated selections within that run can hit derivatives created earlier in the same run.
- **Warm:** the library already holds every photo, its grid rendition, **and the exact deterministic large-preview request sequence** before a fresh GUI process starts. Every requested large derivative must report a hit. Decoded images are not preloaded into the new process.
- **GUI scope:** a fixed 30-second browsing window while imports continue, not a complete cold import. Larger cold libraries show placeholders when scrolling outpaces thumbnail production. The warm workload exercises fully populated rows. Pipeline mode measures complete thumbnail imports.
- **Window behavior:** GUI benchmarks open a non-activating panel at the pop-up window level (`App::open_inactive`, `MacWindowOptions::nonactivating_panel`, level 101), always on top, and do not repeatedly force the window to the front. Rune skips frames for a covered window, so this keeps frames coming when another normal window is active, without taking keyboard focus. The popup can appear above normal windows and across Spaces, so use pipeline mode when that visual intrusion is not acceptable. The harness still requires an unlocked, awake display because a hidden or asleep display cannot prove presentation. Passing does not prove that pixels reached the display. `caffeinate` keeps the Mac awake but does not take focus.
- Benchmark mode uses four columns regardless of width, disables manual selection and web-size preparation, scrolls automatically, requests a preview every three seconds, and sustains frame demand. An independent deadline ends the run even when rendering stops. Failed runs remain failed.
- File readiness ends when the generated derivative reaches the GUI. Decoded readiness ends when `rune::ImageCache` finishes the decode; a repeated preview that is already decoded counts at once. **Neither proves pixels reached the display.** Draw p95 is the CPU time Rune reports per drawn frame (update plus encode), not GPU time. The render-gap keys measure the time between polls, which Rune runs once before every frame.
- Frames come from `App::on_frame_encoded`, keeping only `Drawn` frames. Intervals are the gaps between frame starts; a display-paced window starts each frame on a refresh. They are not physical display scanout. Rune presents every frame it encodes, so the superseded and pending draw counts are always zero, and coverage requires ordered frame starts inside the run. Startup, tail, and maximum presentation gaps each have a 250 ms limit. Missing intervals are null and fail the gate.
- RSS samples sum the app and encoder children every 100 ms. Short-lived peaks can be missed; shared pages can be counted more than once. An unsampled run reports null, not zero memory.

### Budgets

| Metric | Limit |
| --- | ---: |
| Draw p95 | 8.33 ms |
| Present interval p95 / p99 | 20 / 33.34 ms |
| Large-preview file readiness p95, cold / warm | 1500 / 200 ms |
| Sampled process-family RSS | 512 MiB |
| Last-render age / maximum inter-render gap | 250 / 250 ms |
| First presentation / last-presentation age / maximum presentation gap | 250 / 250 / 250 ms |
| Thumbnail / viewer cache entries | 64 / 2 |
| Thumbnail / viewer in-flight decodes | 4 / 1 |

A pass also requires more than 100 measured presentation intervals, complete ordered frame coverage, at least three previews, matching attempted/file-ready/decoded counts, no import/preview/decode failures, and all warm requests hitting large derivatives. Decoded latency is reported separately, without a separate pass budget. These are prototype targets for this Mac, not universal guarantees.

## Measured evidence

On November 16, 2026, the Rune build at `e37cf09` ran the GUI gate on **Apple M1 Pro (MacBookPro18,3), 32 GiB, macOS 27.0 (26A428)**, Rust **1.97.1**, libvips **8.18.3**, through the factory lease `PAL-2324:test:2102310` (Xcode slot, no simulator). The photos are the owner's JPEGs under `~/Pictures`: 291 files, median 25.8 MB. The 20-, 200- and 291-photo samples hold 0.49, 4.74 and 6.91 GB. Each row is one 30-second run; the 1-minute load average was under 8 at the start of each run, with no other lease, simulator, or `xcodebuild` on the Mac.

| Photos | Condition | Draw p95 ms | Present p95 / p99 ms | First present / max present gap ms | Preview-ready p95 ms | Previews ready | RSS MiB | Result |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 20 | Cold | 2.09 | 17.36 / 24.89 | 378.89 / 378.89 | 766.65 | 10 of 11 | 458.4 | Fail |
| 20 | Warm | 2.78 | 17.82 / 25.62 | 166.03 / 166.03 | 17.66 | 11 of 11 | 224.2 | Pass |
| 200 | Cold | 1.22 | 17.04 / 24.79 | 158.59 / 158.59 | 866.65 | 10 of 11 | 509.7 | Fail |
| 200 | Warm | 3.06 | 17.46 / 25.67 | 170.02 / 170.02 | 33.46 | 11 of 11 | 286.8 | Pass |
| 291 | Cold | 1.20 | 16.75 / 24.22 | 121.32 / 121.32 | 882.86 | 10 of 11 | 481.0 | Fail |
| 291 | Warm | 2.78 | 17.04 / 25.16 | 164.97 / 164.97 | 33.12 | 11 of 11 | 279.8 | Pass |
| 200, uploads | Cold | 0.67 | 16.74 / 24.79 | 154.16 / 154.16 | 249.97 | 10 of 11 | 394.0 | Fail |

Every warm run passed every budget and check: all eleven preview requests hit their large derivative, and the 200- and 291-photo runs reached the 64/2 cache and 4/1 decode limits without exceeding them.

Every cold run failed the same check: eleven preview requests and ten ready previews, because the last request was still pending when the 30-second window ended. The earlier GPUI and ZUI builds failed their cold runs the same way. The 20-photo cold run also failed two budgets: its first presentation came at 378.89 ms, against 250 ms, so its maximum presentation gap was also 378.89 ms. It started 2 minutes after the release build, at a 1-minute load of 7.41. The 200-photo cold run stayed 2.4 MiB under the 512 MiB RSS limit. In 30 seconds the cold window imported 20, 78 and 79 photos for the 20-, 200- and 291-photo runs; warm preparation then imported the rest in 44.0 s and 69.7 s with no failures.

The upload run sent each photo to a local Worker that held none of them, while the window ran. It imported 19 photos in 30 seconds; at the end, 17 were in the cloud, 2 were pending and none had failed (`transferred` 17), so the upload check passed. Its only failure was the pending eleventh preview.

Draw p95 stayed under 3.1 ms and present p95 under 18 ms in every run. Raw reports stay on the Mac; they are not in Git.

### Native UI check

Any flag other than `--native` runs the window headless with the same renderer and a demo library: made-up photos with generated pixels, and no library on disk. So a render never shows real photos, and needs no import first. Each state the window can show has a named scene in `src/ui/demo.rs`:

```sh
B=apps/desktop/target/debug/photo-desktop
$B --list-scenes
$B --snap grid.png --scene grid --at 1.5
$B --snap viewer.png --scene viewer --theme light --at 1.5
$B --snap narrow.png --scene grid --size 640x480 --at 1.5
echo '{"events": [{"t": 0.2, "type": "key", "name": "Right"}, {"t": 0.5, "type": "key", "name": "Enter"}]}' > keys.json
$B --script keys.json --snap stepped.png --at 1.5
$B --tree tree.json --scene failures --at 1.5
```

The upload scenes are `uploading`, `upload-draft`, `upload-failed`, `upload-failures`, and `not-connected`. The publish scenes are `published` (the **Published** destination open), `drafts` (the **Drafts** destination open), `multi-select` (four photos selected), `publishing`, `unpublishing`, `publish-failed`, `unpublish-failed`, `cloud-problems`, `description-edit`, and `description-long`. In the demo library, Upload, Publish, and Unpublish change nothing and start no thread; a saved description stays in memory.

Render at `--at 1.5`: Rune animates elements in, so earlier frames show them half in. `cargo test --features desktop` also presses the real view (`src/ui/tests.rs`) and compares every scene's element tree with `goldens/<scene>.txt`. After a change you meant to make, rewrite them with `PHOTO_PROTOTYPE_UPDATE_GOLDENS=1 cargo test --features desktop goldens`, read the diff, and commit them with the change. A refactor rewrites none. `scripts/check` runs format, file size, clippy with no warnings, and every test and golden.

A headless run moves the wall clock and the animation clock together, so anything driven by time (the poll's wakes, the performance panel's refresh) also needs a real window. The benchmark above is that check.

Library events do not wait for the poll's timed wakes. macOS can hold back the timers of a covered or background window, so each event the library sends calls the window's Rune `Waker` (`Library::set_waker`), and `poll` drains the channel at once. A burst of events wakes the window once, until `poll` finds the channel empty. The cloud state of a photo reaches the window only in `CloudChanged`: a record the library sends for another reason keeps the state the window holds, because it may have been read before the last change.

## Design system

The window keeps the Library design from the ZUI build (#21), drawn with Rune and rune-kit. `src/ui/look.rs` holds its palette for each appearance (`docs/photo-gallery` has the references) and lays it over rune-kit's tokens, so kit controls and the app's own parts agree:

- **Shell.** One 44 pt title bar across the window: the sidebar toggle past the traffic lights, the grid's title where the grid starts, and the zoom slider, the Info and performance toggles, and **Import** at the trailing edge. Below it, the sidebar, the grid and the inspector. The sidebar has one destination per collection the library keeps, with its count, and the weight of the originals in its footer: **All photos**, then, while the photo API is connected, **Drafts** (uploaded, not yet public, including a photo being published) and **Published** (public, including a photo being taken out). A destination shows only its photos in the grid and marks itself selected; **All photos** shows every photo. The counts change as soon as a photo's cloud state does. The status bar is mono, shows what is in view and selected at rest, and owns transient import state, so no datum appears twice. Benchmark mode hides the sidebar, because the scripted scroll assumes a fixed column count.
- **Planes.** The grid and the viewer sit on the opaque content plane (`#060606` dark, white light). The title bar, sidebar, inspector and status bar sit on the chrome plane, with a hairline between planes. The accent is red (`#E5483A` dark, `#C8372A` light) and marks only the primary action and progress.
- **Tiles.** Square crops on a plate, 6 pt apart, with a selection ring set 3 pt outside each selected photo. Rune paints hover and press itself, so the pointer crossing tiles never re-runs the view. A photo whose original is missing turns grey and says so. The grid's scroll edges fade while content is hidden under them.
- **Inspector.** 300 pt wide: the preview, the name and size, the camera-settings readout, the film chip, a cloud section with the photo's cloud state, its Upload, Publish, Unpublish, or Retry action, who can see it, and its description, and a detail list that leaves out values the camera never wrote.
- **Viewer and sheet.** The viewer is modal: the photo fitted whole, the camera settings in a pill over its lower edge, step buttons at its sides, and a filmstrip of neighbours below. Escape closes it, and focus returns to the window. The failures sheets slide in at the trailing edge. One lists each file that didn't import, and the other each upload, publish, or unpublish that stopped, with its reason.
- **Type and icons.** Geist and Geist Mono (OFL, `fonts/`), with tabular figures for counts, sizes and camera settings. Icons are Lucide, as SVG bodies (`src/ui/icons.rs`).
- **Numbers from the kit.** Radii come from `k.radius`. Status words appear only for transient import state or when something went wrong.

## Code layout

- `src/ui/gallery/` holds the window's state and the actions that change it. `layout()` computes the grid's columns and tile size, because the grid is virtualised by row and needs exact row heights.
- `src/ui/look.rs` holds the palette, the fonts and the text styles.
- `src/ui/view/` has one file per window region: title bar, grid, inspector, viewer, and the failures sheet. `view/mod.rs` assembles them with the sidebar and the status bar.
- `src/ui/view/parts.rs` holds pieces that take data and draw it (the exposure readout and pill, the film chip, the detail list). They hold no state.
- `src/api.rs` is the one door to the photo API, `src/cloud.rs` holds the cloud state, `src/library/transfer.rs` runs the transfer lanes, `src/library/upload.rs` prepares and sends uploads, and `src/library/publish.rs` holds publish, unpublish, the description, and the check on open. `src/ui/gallery/cloud.rs` holds the window's cloud actions, `src/ui/gallery/description.rs` the description being edited, and `src/ui/view/cloud.rs` draws the inspector's cloud section and the tile mark.
- `src/ui/demo.rs` holds the demo library and the scenes, and `goldens/` each scene's element tree.

## Pipeline choice and remaining product work

**libvips** was selected, and the Rust `image` pipeline has since been removed. The `image` crate now only reads image headers for dimensions. Tests also use it to write JPEG fixtures with EXIF and ICC data. Complete cold imports of twenty evenly sampled JPEGs measured Rust **2.19 thumbnails/sec / 357 MiB** sampled family RSS versus libvips **3.90/sec / 58 MiB**. Both produce max-480-pixel JPEG thumbnails at quality 85; encoder quality scales and resize filters are not equivalent. The Rust `image`/zune-jpeg baseline applies orientation and a triangle resize, without full ICC conversion. Libvips provides shrink-on-load, orientation handling, and sRGB conversion. This is a workflow comparison, not an equal-quality codec benchmark.

Remaining product work:

- Bundle the supported libvips library or helper, and audit licenses, before distribution.
- Tune the WebP quality with real photos and settle on a final value.
- Add derivative quota and cleanup of orphaned files.
- Verify the grid tile names, selection state, and activation actions with VoiceOver. Rune exposes elements through AccessKit, but screen-reader behavior is unverified.
- Verify the reconcile limit (50 pages) against a library larger than 10,000 photos.
- Let a person cancel an upload.

Cargo also reports a future-incompatibility warning in upstream `block` 0.1.6. No budget was relaxed for the Rune port. See the saved [reference](../../docs/photo-gallery/references.md); the video was not inspected, so no visual requirements were inferred from it.

## Camera film simulation export

Imports preserve recognized Fujifilm film simulations from JPEG MakerNote EXIF. The inspector shows the recorded camera setting, such as Classic Chrome or Astia. It does not infer a simulation from a Lightroom profile or claim that the exported pixels use that setting. Missing, unsupported, and malformed camera tags stay absent. The in-process parser follows ExifTool's FujiFilm.pm mappings and handles Fuji's little-endian MakerNote offsets independently of TIFF byte order. It does not require ExifTool at runtime.

Schema v2 adds the film value and a separate checked marker. Existing catalogs backfill one photo per background job after previews, imports, and grid repairs. Each job verifies the source's BLAKE3 identity and checks file identity and modification stamps around EXIF extraction. Missing or changed sources remain pending until a later open or import. Reading uses one opened file and a streaming hash, without buffering the entire JPEG. Originals and metadata-free derivatives stay unchanged.

Explicitly export public records from a catalog:

```sh
cargo run --locked --manifest-path apps/desktop/Cargo.toml --bin export-film-metadata -- \
  /path/to/library /path/to/film-metadata.json
```

The command drains pending film jobs and writes `{ "version": 1, "photos": [...] }` through an atomic file replacement. Each record contains only `id`, `capturedAt: null`, `state: { "_tag": "Draft" }`, and optional `filmSimulation`. It omits paths, local capture times, GPS, serial numbers, camera/lens details, and arbitrary EXIF/XMP. If any legacy photo remains unchecked because its originals are missing or changed, export fails without writing an incomplete file. Reconnect or reimport those originals, then retry.

This is a handoff to the API contract probe. Uploads have their own path; see [Upload](#upload).

On September 30, 2026, all 94 authorized `japan-v2` JPEGs imported without failures. Extraction matched ExifTool's 22 Classic Chrome and four Astia values; the remaining 68 records omitted film simulation. All 94 exported records survived the local HTTP probe and Rust client round-trip. Original SHA-256 hashes stayed unchanged, and all 94 derivatives contained no EXIF, XMP, or ICC chunks. A copied v1 catalog of 223 `japan25` photos migrated and completed its absent-metadata backfill in 4.60 seconds; a repeated export took 0.17 seconds. These timings measure the CLI backfill and export. Local evidence remains under `benchmarks/local/film-simulation-20260930`.

### Real JPEG performance evidence

The retained September 29, 2026 japan25 run used 223 JPEGs totaling 5.5 GB. After the 30-second cold window, only 59 thumbnails were ready. Draw p95 was 1.43 ms, presentation p95/p99 was 17.34/18.31 ms, preview latency was 1008.29 ms, sampled family RSS was 661.97 MiB, maximum draw gap was 449.93 ms, and maximum presentation gap was 449.55 ms. The cold run failed the memory and gap budgets and did not complete import.

The warm run had all 223 thumbnails ready. Draw p95 was 1.60 ms, presentation p95/p99 was 17.34/17.89 ms, preview latency was 14.63 ms, sampled family RSS was 305.80 MiB, and maximum draw gap was 117.21 ms. It passed. The synthetic results above do not supersede this real cold failure. Local evidence remains under `benchmarks/local/japan25-223-20260929` and stays ignored.
