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

Selecting a folder imports JPEGs from all nested subfolders into one grid. Files in different folders can share a filename. Selecting overlapping folders or files imports each canonical source path once. Folder names do not become albums.

Originals are read-only and never uploaded. Cached JPEGs live in `~/Library/Caches/dev.parth.photo-prototype/v1`, overridable with `PHOTO_CACHE_DIR`. Photos library packages and Photo Booth are excluded; symlinks are skipped. Other unreadable folders fail the scan with an error.

## Processing and memory bounds

The grid virtualizes four-column rows. Encoded 480-pixel thumbnails and 2400-pixel previews stay on disk. GPUI caches retain at most **64 thumbnail / 2 viewer entries**, with at most **4 / 1 in-flight decodes**. Pending loads are settled before eviction, including loads that scroll off-screen; reports measure the underlying cache entries, not just an outer list of keys. Available thumbnails remain visible until a larger preview arrives. Image work stays off the UI thread.

One thumbnail lane and one interactive-preview lane allow at most two encoder jobs. New thumbnails wait while an interactive request is pending/running, but an already-running thumbnail cannot delay the separate preview lane. The latest pending preview replaces older pending requests. Tests cover recursive imports, overlapping selections, unchanged originals, reuse, invalid JPEGs, image bounds, and interactive priority behind an expensive import.

## Verify

```sh
cargo fmt --manifest-path apps/desktop/Cargo.toml --all -- --check
cargo test --locked --manifest-path apps/desktop/Cargo.toml
cargo check --locked --manifest-path apps/desktop/Cargo.toml --features desktop
cargo build --release --locked --manifest-path apps/desktop/Cargo.toml --features desktop --bins
python3 apps/desktop/benchmark.py "$HOME/Pictures" --mode pipeline --count 20 --output apps/desktop/benchmarks/local/pipeline-run
caffeinate -dimsu python3 apps/desktop/benchmark.py "$HOME/Pictures" --count 200 --seconds 30 --keep-active --output apps/desktop/benchmarks/local/gui-run
```

Repeat the GUI command with counts 20, 200, and the full authorized library. Output directories must be new. The harness samples evenly across the folder tree and omits source names/paths from JSON. Caches and raw reports are ignored by Git. Encoder stderr and scene captures can contain private material; keep them local.

### Conditions and measurement

- **Cold:** empty derivative cache, not a flushed OS file cache. Repeated selections within that run can hit derivatives created earlier in the same run.
- **Warm:** complete all thumbnails **and the exact deterministic large-preview request sequence** before starting a fresh GUI process. Every requested large derivative must report a hit. Decoded GPUI images are not preloaded into the new process.
- **GUI scope:** a fixed 30-second browsing window while imports continue, not a complete cold import. Larger cold libraries show placeholders when scrolling outpaces thumbnail production. The warm workload exercises fully populated rows. Pipeline mode measures complete thumbnail imports.
- **Foreground:** unlock the Mac, wake its display, and keep the window visible. The harness checks these display prerequisites before launch; window activation cannot override a locked session. `--keep-active` restores activation **and window ordering every 100 ms**, even when the window reports itself active. This opt-in control steals focus during timed runs; do not interact with other apps. Ordinary interactive use never forces focus. It is a controlled foreground measurement, not a guarantee about occluded/background rendering.
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

**Gate status: BLOCKED.** Six earlier controlled runs passed, but the final post-review warm run did not. Keep #3 open until the render-gap failure is explained and the workload meets its budgets reliably. Do not select a favorable run as proof of repeatability.

Measured on **Apple M1 Pro, 32 GiB, macOS 26.6.2**, Rust **1.97.1**, libvips **8.18.3**, using owner-authorized JPEGs under Pictures. The full tree contains **944 JPEGs / 17.35 GB**, median **20.92 MB**. Twenty- and 200-photo samples contain 368 MB and 3.74 GB respectively. Lightroom provenance was supplied by the owner, not independently established.

The earlier controlled-foreground runs (`gui-{20,200,944}-ordered` locally) each passed the unchanged numerical budgets:

| Photos | Cache | Thumbnails complete | Draw p95 ms | Present p95 / p99 ms | File / decoded p95 ms | RSS MiB |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 20 | Cold | 20 | 0.31 | 8.94 / 9.06 | 555.6 / 580.1 | 281.9 |
| 20 | Warm | 20 | 0.31 | 9.26 / 12.99 | 24.4 / 41.6 | 183.5 |
| 200 | Cold | 87 | 0.43 | 9.22 / 9.60 | 571.5 / 599.8 | 359.1 |
| 200 | Warm | 200 | 0.48 | 8.99 / 9.18 | 20.9 / 50.1 | 270.3 |
| 944 | Cold | 74 | 0.32 | 9.02 / 9.44 | 605.8 / 625.1 | 337.6 |
| 944 | Warm | 944 | 0.50 | 9.16 / 9.50 | 20.0 / 49.8 | 263.4 |

Each run completed ten of ten preview requests with no failures. Maximum render gaps were 58–71 ms. Warm runs hit all ten derivatives and reached the 64/2 cache and 4/1 loading bounds on larger libraries. Full-library warm preparation completed all 944 thumbnails without failures; it took 251.2 seconds after the cold window had already made 74 thumbnails. This is mixed-cache preparation, not a clean cold-throughput measurement.

Earlier failures informed fixes and were not discarded: thumbnail-only warming produced false warm requests; a serial encoder delayed previews beyond 1.5 seconds; key-window-only activation left long no-render gaps (up to 8.53 seconds in the later 20-photo run). Exact derivative warming, independent priority processing, bounded pending decodes, and restoring window ordering enabled the six passing runs, but did not establish repeatability.

After review fixes, `gui-20-reviewed` passed the unlocked/awake display preflight. Its cold run passed, but warm recorded a **1030.81 ms maximum render gap** before frame 68, over the **250 ms** limit. All ten previews completed: file/decoded p95 **23.83 / 205.94 ms**, draw p95 **0.29 ms**, present p95/p99 **8.93 / 9.23 ms**, RSS **182.7 MiB**. Favorable percentiles do not cancel the render-gap failure. Zero recorded inactive frames does not rule out startup focus loss: that counter starts after two seconds. The remaining GPUI/Metal/window-activation cause is not established. A separate earlier rerun had no frame delivery because the Mac was locked/asleep; the new preflight rejects that condition before creating caches. No display restriction was bypassed.

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
