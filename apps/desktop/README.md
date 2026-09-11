# Photo prototype

Local-only GPUI performance gate for [issue #3](https://github.com/ParthMmm/web2026/issues/3). This is not the finished photo manager: no database, authentication, uploads, editing, or public publishing.

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

Alternatively launch without an argument and choose **Import JPEGs…**. One import session per process; restart to choose another library. Select a thumbnail to open a larger preview. Cmd-Q quits. **Show/Hide performance** toggles gpui-fps. Right-click its headline to switch MAX FPS (estimated sustainable redraw rate) to presented FPS; click to collapse it. Its default grading budget is 60 Hz.

On macOS 13 and later, choose **Import from Photos…** to open the native Photos picker. The picker reads only the assets you select; it does not open the Photos library package or database. JPEG representations are copied into app-managed storage, and other image representations are converted to JPEG with the system `sips` tool. The selected files are copied while the provider's temporary URLs are valid. Cancelled or failed selections are removed. The default import location is `~/Library/Application Support/dev.parth.photo-prototype/imports/v1`; set `PHOTO_IMPORT_DIR` to override it. The copied files then use the same local JPEG import and preview pipeline as file selections.

Selecting a folder imports JPEGs from all nested subfolders into one grid. Files in different folders can share a filename. Selecting overlapping folders or files imports each canonical source path once. Folder names do not become albums.

Originals are read-only and never uploaded. Cached JPEGs live in `~/Library/Caches/dev.parth.photo-prototype/v1`, overridable with `PHOTO_CACHE_DIR`. Photos library packages and Photo Booth are excluded; symlinks are skipped. Other unreadable folders fail the scan with an error.

## Processing and memory bounds

The grid virtualizes four-column rows. Encoded 480-pixel thumbnails and 2400-pixel previews stay on disk. GPUI caches retain at most **64 thumbnail / 2 viewer entries**, with at most **4 / 1 in-flight decodes**. Pending loads are settled before eviction, including loads that scroll off-screen; reports measure the underlying cache entries, not just an outer list of keys. Available thumbnails remain visible until a larger preview arrives. Image work stays off the UI thread.

One thumbnail lane and one interactive-preview lane allow at most two encoder jobs. New thumbnails wait while an interactive request is pending/running, but an already-running thumbnail cannot delay the separate preview lane. The latest pending preview replaces older pending requests. Tests cover recursive imports, overlapping selections, unchanged originals, reuse, invalid JPEGs, image bounds, and interactive priority behind an expensive import.

## Verify

```sh
cargo fmt --manifest-path apps/desktop/Cargo.toml --all -- --check
cargo test --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo check --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo build --release --locked --manifest-path apps/desktop/Cargo.toml --features desktop --bins
python3 apps/desktop/benchmark.py "$HOME/Pictures" --mode pipeline --count 20 --output apps/desktop/benchmarks/local/pipeline-run
caffeinate -dimsu python3 apps/desktop/benchmark.py "$HOME/Pictures" --count 200 --seconds 30 --output apps/desktop/benchmarks/local/gui-run
```

Repeat the GUI command with counts 20, 200, and the full authorized library. Output directories must be new. The harness samples evenly across the folder tree and omits source names/paths from JSON. Caches and raw reports are ignored by Git. Encoder stderr and scene captures can contain private material; keep them local.

### Conditions and measurement

- **Cold:** empty derivative cache, not a flushed OS file cache. Repeated selections within that run can hit derivatives created earlier in the same run.
- **Warm:** complete all thumbnails **and the exact deterministic large-preview request sequence** before starting a fresh GUI process. Every requested large derivative must report a hit. Decoded GPUI images are not preloaded into the new process.
- **GUI scope:** a fixed 30-second browsing window while imports continue, not a complete cold import. Larger cold libraries show placeholders when scrolling outpaces thumbnail production. The warm workload exercises fully populated rows. Pipeline mode measures complete thumbnail imports.
- **Window behavior:** GUI benchmarks use a non-activating popup-level window. They set `focus = false`, do not call `cx.activate`, and do not repeatedly force the window to the front. This keeps GPUI 0.3.4's macOS frame source running when another normal window is active, without taking keyboard focus. The popup can appear above normal windows and across Spaces, so use pipeline mode when that visual intrusion is not acceptable. The harness still requires an unlocked, awake display because a hidden or asleep display cannot prove presentation. Passing does not prove that pixels reached the display. `caffeinate` keeps the Mac awake but does not take focus.
- Benchmark mode disables the HUD/manual selection, scrolls automatically, requests a preview every three seconds, and sustains frame demand. An independent deadline ends the run even when rendering stops. Failed runs remain failed.
- File readiness ends when the generated derivative reaches the GUI. Decoded readiness ends when GPUI's image cache resolves it. **Neither proves pixels reached the display.** Draw and present-interval histograms are not GPU execution timings. The maximum gap measures time between Gallery render calls; initial render invocation is reported separately.
- RSS samples sum the app and encoder children every 100 ms. Short-lived peaks can be missed; shared pages can be counted more than once. An unsampled run reports null, not zero memory.

### Budgets

| Metric | Limit |
| --- | ---: |
| Draw p95 | 8.33 ms |
| Present interval p95 / p99 | 20 / 33.34 ms |
| Large-preview file readiness p95, cold / warm | 1500 / 200 ms |
| Sampled process-family RSS | 512 MiB |
| Last-render age / maximum inter-render gap | 250 / 250 ms |
| Thumbnail / viewer cache entries | 64 / 2 |
| Thumbnail / viewer in-flight decodes | 4 / 1 |

A pass also requires more than 100 draws, at least three previews, matching attempted/file-ready/decoded counts, no import/preview/decode failures, and all warm requests hitting large derivatives. Decoded latency is reported separately, without a separate pass budget. These are prototype targets for this Mac, not universal guarantees.

## Measured evidence

**Gate status: PASS for the current 30-second GUI workload.** The earlier `gui-20-reviewed` warm failure is retained as regression evidence, not replaced by a favorable result. The render-gap cause is now understood: pinned GPUI 0.3.4 stops its macOS `CVDisplayLink` source when a normal window is fully occluded, even when the window is dirty and inactive-window throttling is disabled.

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

Inspected actual app-owned **2400×1640 Metal scene captures** with real images. Native pointer events selected a photo and toggled the FPS HUD off. Captures confirmed aligned columns and fitted portrait/landscape images; the sizing check caught and fixed intrinsic-image overflow. Native Cmd-Q through System Events exited successfully. These are scene/interaction checks, not OS screenshots or frame-presentation proofs. Screen-recording capture was denied and was not required.

For local visual inspection, `visual-check` enables GPUI's test-support scene renderer. It writes only this app's scene at four and eight seconds; interact during that interval, then quit normally:

```sh
PHOTO_SNAPSHOT_DIR="$PWD/apps/desktop/benchmarks/local/visual-check" \
  cargo run --release --locked --manifest-path apps/desktop/Cargo.toml --features visual-check --bin photo-desktop -- /path/to/authorized/jpeg-folder
```

Never commit these captures. Production benchmark binaries use `--features desktop`, without `visual-check`.

## Pipeline choice and remaining product work

Select **libvips**. Complete cold imports of twenty evenly sampled JPEGs measured Rust **2.19 thumbnails/sec / 357 MiB** sampled family RSS versus libvips **3.90/sec / 58 MiB**. Both produce max-480-pixel JPEG thumbnails at quality 85; encoder quality scales and resize filters are not equivalent. The Rust `image`/zune-jpeg baseline applies orientation and a triangle resize, without full ICC conversion. Libvips provides shrink-on-load, orientation handling, and sRGB conversion. This is a workflow comparison, not an equal-quality codec benchmark.

Before distribution, bundle the supported libvips library or helper, audit licenses, and verify ICC/orientation fixtures. Cache validation currently checks image headers/dimensions, not a full decode; its path/size/mtime identity is not content deduplication or protection against in-place source changes. Production color/privacy fixtures, cache recovery/quota, content identity, SQLite, keyboard grid navigation, and accessible controls remain product work. Clickable prototype divs are not a finished accessibility system. Cargo also reports a future-incompatibility warning in upstream `block` 0.1.6.

Native browsing and the behavior-level tests pass; the performance gate remains blocked by the intermittent render gap. No framework switch or budget relaxation was made. See the saved [reference](../../docs/photo-gallery/references.md); the video was not inspected, so no visual requirements were inferred from it.
