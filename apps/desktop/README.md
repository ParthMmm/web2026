# Photo prototype

Local-only macOS photo library built with GPUI Kit. It implements the persistent local library from [issue #5](https://github.com/ParthMmm/web2026/issues/5) on top of the performance gate from [issue #3](https://github.com/ParthMmm/web2026/issues/3). There is no authentication, upload, editing, or public publishing yet.

## Run on macOS

Requires Xcode with its Metal toolchain, Rust, and **libvips 8.18.3**. The app checks `vips --version` before any Vips import or preview, rejects other versions, and caches a successful check for the process. GPUI Kit and gpui-fps are pinned to **0.6.1**; `Cargo.lock` selects their matching GPUI pre-release **0.3.4** packages.

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

Arguments are JPEG files or folders to import on launch. You can also launch without arguments and use **Import…** (Cmd-O) or **Import from Photos…** (Cmd-Shift-O), from the toolbar or the File menu. Imports queue, so you can start another one while one is running. The status bar shows progress and a one-line summary when the import finishes.

- **Browse:** click a photo to select it, or use the arrow keys, Page Up/Down, and Home/End (or Cmd-Up/Down). The inspector shows the large preview, dimensions, file size, camera, lens, exposure, capture time, and the original's location.
- **View:** double-click, Space, or Return opens the full-window viewer. Left/Right step through photos; Escape or Space closes it and returns focus to the grid.
- **Failures:** files that couldn't be imported appear under **N not imported…** in the toolbar (also File › Files not imported…). The sheet shows the reason for each file, and offers **Retry all** and **Clear list**.
- **Performance:** the chip button (Cmd-Alt-P, or View › Show or hide performance) toggles gpui-fps. Right-click its headline to switch MAX FPS (estimated sustainable redraw rate) to presented FPS; click to collapse it. Its default grading budget is 60 Hz.

The theme follows the system's light or dark appearance.

On macOS 13 and later, **Import from Photos…** opens the native Photos picker. The picker reads only the assets you select; it does not open the Photos library package or database. JPEG representations are copied into the library's `imports/` folder, and other image representations are converted to JPEG with the system `sips` tool. Those copies become the photos' originals. Copies that duplicate a photo already in the library, or that fail to import, are deleted. Set `PHOTO_IMPORT_DIR` to store copies elsewhere.

Selecting a folder imports JPEGs from all nested subfolders. Photos library packages and Photo Booth are excluded, and symlinks are skipped. Folder names do not become albums.

## The library

The library lives in `~/Library/Application Support/dev.parth.photo-prototype/library`; set `PHOTO_LIBRARY_DIR` to use another one. It contains a SQLite catalog (`catalog.sqlite`) and generated `derivatives/`. Originals are only read, never modified, moved, or uploaded.

- **Identity and duplicates.** A photo's identity is the BLAKE3 hash of its bytes. Importing the same bytes from another path adds a known location instead of a second photo. The inspector shows "Found in N places". Re-importing a known, unchanged path (same size and modification time) skips hashing entirely.
- **Missing and moved originals.** On open, sources that no longer exist are flagged. The grid marks a photo whose every known original is missing, and the inspector explains how to reconnect it: import the file from its new location. Existing previews stay available. If a file's content changes in place, it becomes a new photo. The old photo is removed once none of its sources still hold its bytes.
- **Failures.** Each failed file is recorded with a readable reason, for example "Not a readable JPEG" or "The JPEG is incomplete or damaged". A later successful import clears the record. Failures persist across launches until retried or cleared.
- **Derivatives.** libvips produces WebP renditions at 480 px (grid), 1200 px (mobile), and 2400 px (desktop) on the long edge, never upscaled. Renditions apply EXIF orientation, are converted to sRGB from the embedded ICC profile, and carry no metadata: no EXIF, GPS, ICC, or XMP chunks. The grid rendition is made during import. The larger ones are prepared in the background and on demand for the viewer. Recipe version 1 uses quality 82, which is provisional. A changed recipe version or a missing file is regenerated.
- **Metadata.** ISO, exposure time, aperture, focal length, camera, lens, and capture time (with its UTC offset, when recorded) are read locally from EXIF and stored in the catalog. GPS and serial numbers are ignored. Malformed metadata never fails an import.

## Processing and memory bounds

The grid virtualizes rows. Columns follow the window width (at least 168 px per tile), and the inspector collapses below 760 px. Encoded renditions stay on disk. GPUI caches retain at most **160 thumbnail / 2 viewer entries** interactively (**64 / 2** in benchmark mode), with at most **4 / 1 in-flight decodes**. Pending loads are settled before eviction, including loads that scroll off-screen. Available thumbnails stay visible until the larger preview arrives. Image work stays off the UI thread.

Two worker lanes allow at most two encoder processes. The background lane runs, in priority order: clearing failures, scanning, importing, grid repairs, then larger web sizes. It waits while an interactive preview is pending or running. The interactive lane renders the selected photo's desktop rendition. The latest pending preview replaces older ones. A running background job is never cancelled midway through a write.

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
cargo test --locked --manifest-path apps/desktop/Cargo.toml --features visual-check --lib failures_sheet
cargo check --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo build --release --locked --manifest-path apps/desktop/Cargo.toml --features desktop --bins
python3 apps/desktop/benchmark.py "$HOME/Pictures" --mode pipeline --count 20 --output apps/desktop/benchmarks/local/pipeline-run
caffeinate -dimsu python3 apps/desktop/benchmark.py "$HOME/Pictures" --count 200 --seconds 30 --output apps/desktop/benchmarks/local/gui-run
```

Repeat the GUI command with counts 20, 200, and the full authorized library. Output directories must be new. Each run creates its own library inside the output directory, so benchmarks never touch your real library. The harness samples evenly across the folder tree and omits source names and paths from JSON. Libraries and raw reports are ignored by Git. Encoder stderr and scene captures can contain private material; keep them local.

`benchmark-pipeline <library-directory> <JPEG-or-folder>...` imports without a window and prints one JSON summary. `PHOTO_PREWARM_INDICES` also renders the desktop previews for those grid positions, which the warm GUI run requires.

### Conditions and measurement

- **Cold:** empty library, not a flushed OS file cache. Repeated selections within that run can hit derivatives created earlier in the same run.
- **Warm:** the library already holds every photo, its grid rendition, **and the exact deterministic large-preview request sequence** before a fresh GUI process starts. Every requested large derivative must report a hit. Decoded GPUI images are not preloaded into the new process.
- **GUI scope:** a fixed 30-second browsing window while imports continue, not a complete cold import. Larger cold libraries show placeholders when scrolling outpaces thumbnail production. The warm workload exercises fully populated rows. Pipeline mode measures complete thumbnail imports.
- **Window behavior:** GUI benchmarks use a non-activating popup-level window. They set `focus = false`, do not call `cx.activate`, and do not repeatedly force the window to the front. This keeps GPUI 0.3.4's macOS frame source running when another normal window is active, without taking keyboard focus. The popup can appear above normal windows and across Spaces, so use pipeline mode when that visual intrusion is not acceptable. The harness still requires an unlocked, awake display because a hidden or asleep display cannot prove presentation. Passing does not prove that pixels reached the display. `caffeinate` keeps the Mac awake but does not take focus.
- Benchmark mode uses four columns regardless of width, disables manual selection and web-size preparation, scrolls automatically, requests a preview every three seconds, and sustains frame demand. An independent deadline ends the run even when rendering stops. Failed runs remain failed.
- File readiness ends when the generated derivative reaches the GUI. Decoded readiness ends when GPUI's image cache resolves it. **Neither proves pixels reached the display.** Draw and present-interval histograms are not GPU execution timings. The maximum gap measures time between Gallery render calls; initial render invocation is reported separately.
- Frame intervals come from every newly drawn target-window frame submission, including inactive frames. They measure time between `present_end` events, not physical display scanout. Ordered trace accounting includes draws superseded before presentation; a pass requires zero trailing unpresented draws and exact draw coverage. Startup, tail, and maximum presentation gaps each have a 250 ms limit. Missing intervals are null and fail the gate.
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

On September 29, 2026, the corrected full-trace gate passed all four 30-second runs with 24 and 200 generated JPEGs on **Apple M1 Pro, 32 GiB, macOS 27.0 (26A428), libvips 8.18.3**.

| Photos | Cache | Draw p95 ms | Present p95 / p99 ms | Max present gap ms | Preview-ready p95 ms | RSS MiB |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 24 | Cold | 0.95 | 16.98 / 19.81 | 150.52 | 208.98 | 166.44 |
| 24 | Warm | 0.95 | 17.46 / 23.86 | 66.73 | 17.24 | 153.25 |
| 200 | Cold | 1.33 | 17.25 / 20.03 | 150.00 | 277.53 | 268.67 |
| 200 | Warm | 1.46 | 17.28 / 23.88 | 52.57 | 16.60 | 254.64 |

Each run completed ten previews without failures, accounted for one superseded draw, and had zero pending draws. Every warm preview hit its large derivative. Both 200-photo runs reached the 64/2 cache and 4/1 decode limits without exceeding them. Reports remain in `benchmarks/local/fix-20260929/final-24/` and `final-200/`. These synthetic fixtures do not replace the real-photo matrix or establish the cause of earlier frame variation.

The older figures below use GPUI’s activity-filtered presentation histogram and are not directly comparable to the current full-trace intervals. The measurements below are from issue #3, taken before the persistent library replaced the JPEG cache and the UI moved to GPUI Kit components. After those changes, a 24-photo smoke run of the same harness passed every budget. Cold: draw p95 0.93 ms, present p95/p99 17.7/19.9 ms, preview-ready p95 784 ms. Warm: draw p95 1.13 ms, present p95/p99 17.7/33.0 ms, preview-ready p95 13 ms, all four preview requests cache hits. Pipeline mode re-imported the unchanged 24 photos in 0.07 s. Rerun the full matrix below before relying on it for larger libraries.

Earlier checks were not consistently green after the persistent-library changes. On September 29, 2026, two fresh 12-second runs with 24 generated JPEGs each failed frame-interval budgets. The first cold run recorded p95 23.81 ms against 20 ms while a separate pipeline check ran. An isolated rerun passed cold, but warm recorded p95 21.00 ms and p99 33.341 ms against 20 ms and 33.34 ms. All four runs completed every preview without import or decode failures, and draw p95 stayed below 1.4 ms. The cause of the frame-interval variation remains unresolved. Both reports remain locally under `benchmarks/local/pickup-20260929/`. These generated fixtures do not replace the full real-photo matrix.

The earlier 30-second GUI workload passed before the persistent-library changes. The earlier `gui-20-reviewed` warm failure is retained as regression evidence, not replaced by a favorable result. Its render-gap cause is understood: pinned GPUI 0.3.4 stops its macOS `CVDisplayLink` source when a normal window is fully occluded, even when the window is dirty and inactive-window throttling is disabled.

Measured on **Apple M1 Pro, 32 GiB, macOS 26.6.2**, Rust **1.97.1**, libvips **8.18.3**, using owner-authorized JPEGs under Pictures. The full tree contains **944 JPEGs / 17.35 GB**, median **20.92 MB**. Twenty- and 200-photo samples contain 368 MB and 3.74 GB respectively. Lightroom provenance was supplied by the owner, not independently established.

Benchmark mode now uses GPUI's `WindowKind::PopUp`, which maps to a non-activating macOS panel at popup level. It sets `focus = false`, leaves the inactive-window throttle disabled, and does not call `cx.activate` or repeatedly raise the window. This keeps the frame source alive while another normal app is active without taking keyboard focus. The trade-off is intentional and documented: the benchmark window can appear above normal windows and across Spaces. A first post-fix 200-photo warm run still recorded a **1125.08 ms** gap and failed; it was retained. Three direct warm reruns, the next 200-photo harness run, and the full-library run stayed below 52 ms. This is evidence for the fixed path, not a claim that external display or user actions cannot interrupt a run.

The post-fix 30-second harness runs passed the unchanged budgets:

| Photos | Cache | Thumbnails complete | Draw p95 ms | Present p95 / p99 ms | Preview-ready p95 ms | Max gap ms | Last age ms | RSS MiB |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 20 | Cold | 20 | 0.40 | 17.63 / 19.23 | 555.5 | 43.3 | 4.5 | 258.2 |
| 20 | Warm | 20 | 0.37 | 17.63 / 19.27 | 16.0 | 47.0 | 3.0 | 174.6 |
| 200 | Cold | 87 | 0.57 | 17.56 / 17.92 | 588.0 | 44.9 | 6.3 | 342.4 |
| 200 | Warm | 200 | 0.88 | 17.58 / 30.87 | 15.6 | 44.7 | 5.4 | 246.2 |
| 944 | Cold | 73 | 0.51 | 17.61 / 18.24 | 653.8 | 52.5 | 15.3 | 281.1 |
| 944 | Warm | 944 | 0.95 | 17.53 / 20.17 | 18.8 | 44.1 | 3.8 | 239.3 |

Every post-fix run completed ten of ten preview requests with no import, preview, or decode failures. Warm runs hit all ten large derivatives and reached the 64/2 cache and 4/1 loading bounds where the workload required them. Full-library warm preparation completed all 944 thumbnails without failures in 256.8 seconds after the cold window had made 73 thumbnails. This is mixed-cache preparation, not a clean cold-throughput measurement.

Earlier failures informed fixes and were not discarded: thumbnail-only warming produced false warm requests; a serial encoder delayed previews beyond 1.5 seconds; key-window-only activation left long no-render gaps (up to 8.53 seconds); and the pre-fix `gui-20-reviewed` warm run reached 1030.81 ms. The benchmark still rejects locked/asleep displays, reports inactive frames honestly, and does not claim that pixels reached the display. No display restriction was bypassed.

### Native UI check

The September 29, 2026 check found a panic when opening **Not imported…**: the sheet builder read Gallery while Gallery was already being updated during rendering. The builder now captures the failure list before opening the sheet; Retry and Clear still update the live Gallery. A headless GPUI regression test renders the actual Gallery and sheet, verifies Retry/Clear callbacks and focus restoration, and confirms the empty sheet’s controls do nothing. All 32 unit and 15 library tests pass with `visual-check`. The test reproduces the original borrow panic when the old builder is restored. The repaired binary starts and exits cleanly, but the native input driver could not reliably focus its window, so repaired-sheet interaction through macOS input remains unverified.

Inspected actual app-owned **2400×1640 Metal scene captures** with real images. Native pointer events selected a photo and toggled the FPS HUD off. Captures confirmed aligned columns and fitted portrait/landscape images; the sizing check caught and fixed intrinsic-image overflow. Native Cmd-Q through System Events exited successfully. These are scene/interaction checks, not OS screenshots or frame-presentation proofs. Screen-recording capture was denied and was not required.

For local visual inspection, `visual-check` enables GPUI's test-support scene renderer. It writes only this app's scene at four and eight seconds; interact during that interval, then quit normally:

```sh
PHOTO_SNAPSHOT_DIR="$PWD/apps/desktop/benchmarks/local/visual-check" \
  cargo run --release --locked --manifest-path apps/desktop/Cargo.toml --features visual-check --bin photo-desktop -- /path/to/authorized/jpeg-folder
```

Never commit these captures. Production benchmark binaries use `--features desktop`, without `visual-check`.

## Pipeline choice and remaining product work

**libvips** was selected, and the Rust `image` pipeline has since been removed. The `image` crate now only reads image headers for dimensions. Tests also use it to write JPEG fixtures with EXIF and ICC data. Complete cold imports of twenty evenly sampled JPEGs measured Rust **2.19 thumbnails/sec / 357 MiB** sampled family RSS versus libvips **3.90/sec / 58 MiB**. Both produce max-480-pixel JPEG thumbnails at quality 85; encoder quality scales and resize filters are not equivalent. The Rust `image`/zune-jpeg baseline applies orientation and a triangle resize, without full ICC conversion. Libvips provides shrink-on-load, orientation handling, and sRGB conversion. This is a workflow comparison, not an equal-quality codec benchmark.

Remaining product work:

- Bundle the supported libvips library or helper, and audit licenses, before distribution.
- Tune the WebP quality with real photos and settle on a final value.
- Add derivative quota and cleanup of orphaned files.
- Verify the new grid tile names, selection state, and activation actions with VoiceOver. Native accessibility-tree inspection did not expose the GPUI descendants, so screen-reader behavior is unverified.
- Add upload and publishing.

Cargo also reports a future-incompatibility warning in upstream `block` 0.1.6. No framework switch or budget relaxation was made. See the saved [reference](../../docs/photo-gallery/references.md); the video was not inspected, so no visual requirements were inferred from it.

## Camera film simulation export

Imports preserve recognized Fujifilm film simulations from JPEG MakerNote EXIF. The inspector shows the recorded camera setting, such as Classic Chrome or Astia. It does not infer a simulation from a Lightroom profile or claim that the exported pixels use that setting. Missing, unsupported, and malformed camera tags stay absent. The in-process parser follows ExifTool's FujiFilm.pm mappings and handles Fuji's little-endian MakerNote offsets independently of TIFF byte order. It does not require ExifTool at runtime.

Schema v2 adds the film value and a separate checked marker. Existing catalogs backfill one photo per background job after previews, imports, and grid repairs. Each job verifies the source's BLAKE3 identity and checks file identity and modification stamps around EXIF extraction. Missing or changed sources remain pending until a later open or import. Reading uses one opened file and a streaming hash, without buffering the entire JPEG. Originals and metadata-free derivatives stay unchanged.

Explicitly export public records from a catalog:

```sh
cargo run --locked --manifest-path apps/desktop/Cargo.toml --bin export-film-metadata -- \
  /path/to/library /path/to/film-metadata.json
```

The command drains pending film jobs and writes `{ "version": 1, "photos": [...] }` through an atomic file replacement. Each record contains only `id`, `capturedAt: null`, `state: { "_tag": "Draft" }`, and optional `filmSimulation`. It omits paths, local capture times, GPS, serial numbers, camera/lens details, and arbitrary EXIF/XMP. If any legacy photo remains unchecked because its originals are missing or changed, export fails without writing an incomplete file. Reconnect or reimport those originals, then retry.

This is a handoff to the API contract probe. Upload, cloud persistence, and a public gallery remain future work.

On September 30, 2026, all 94 authorized `japan-v2` JPEGs imported without failures. Extraction matched ExifTool's 22 Classic Chrome and four Astia values; the remaining 68 records omitted film simulation. All 94 exported records survived the local HTTP probe and Rust client round-trip. Original SHA-256 hashes stayed unchanged, and all 94 derivatives contained no EXIF, XMP, or ICC chunks. A copied v1 catalog of 223 `japan25` photos migrated and completed its absent-metadata backfill in 4.60 seconds; a repeated export took 0.17 seconds. These timings measure the CLI backfill and export. Local evidence remains under `benchmarks/local/film-simulation-20260930`.

### Real JPEG performance evidence

The retained September 29, 2026 japan25 run used 223 JPEGs totaling 5.5 GB. After the 30-second cold window, only 59 thumbnails were ready. Draw p95 was 1.43 ms, presentation p95/p99 was 17.34/18.31 ms, preview latency was 1008.29 ms, sampled family RSS was 661.97 MiB, maximum draw gap was 449.93 ms, and maximum presentation gap was 449.55 ms. The cold run failed the memory and gap budgets and did not complete import.

The warm run had all 223 thumbnails ready. Draw p95 was 1.60 ms, presentation p95/p99 was 17.34/17.89 ms, preview latency was 14.63 ms, sampled family RSS was 305.80 MiB, and maximum draw gap was 117.21 ms. It passed. The synthetic results above do not supersede this real cold failure. Local evidence remains under `benchmarks/local/japan25-223-20260929` and stays ignored.
