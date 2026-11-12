# AGENTS.md

A local macOS photo library, drawn with [Rune](https://github.com/anomalyco/rune) and rune-kit. It imports JPEGs into a SQLite catalog, makes renditions with libvips, and shows them in a grid, an inspector and a full-window viewer. When it is set up, it uploads photos to the photo API in `apps/api` as private drafts and publishes them; otherwise it talks to no service. `README.md` describes every feature, the benchmark gate, and how to check the window; read the section for the part you change. Rune's own patterns are in the `rune-app` skill (`~/dotfiles/agents/skills/rune-app/`).

## Boundaries

- The person's library (`~/Library/Application Support/dev.parth.photo-prototype/library`) and their Photos library are off limits. Set `PHOTO_LIBRARY_DIR` to a scratch folder for any real-window run, and import only generated or authorized JPEGs.
- The app opens the library only on a plain launch, `--native`, or a benchmark run. Every other flag (`--snap`, `--tree`, `--script`, `--scene`) shows the demo library from `src/ui/demo.rs`: made-up photos, nothing on disk. Never commit a capture of a real library.
- Upload and publish only to a local Worker (`apps/api`, `bun scripts/serve.ts`) or to the fake API in `tests/support/`. Never point `PHOTO_API_URL` at production, and never write the owner token to the catalog, a log, an error or a commit.

## Shape of the code

- `src/ui/gallery/` holds the window's state. Library events land in `events.rs`, and imports and pickers live in `import.rs`. `LibraryState::Demo` is the demo library; it has no `AppHandle` and starts no threads.
- `src/ui/view/` draws that state, one file per part (grid, inspector, overlays). The view reads no clock; `poll` refreshes anything time-driven, such as the performance line.
- `src/api.rs` is the one door to the photo API; nothing else makes a network call. `src/cloud.rs` holds the cloud state each photo carries and which step may start from it. `src/library/transfer.rs` runs the transfer lanes and their retry; `src/library/upload.rs` prepares and sends uploads, and `src/library/publish.rs` publishes, unpublishes, keeps descriptions, and checks the catalog against the server on open. The window's cloud actions live in `src/ui/gallery/cloud.rs`, and the description being edited in `src/ui/gallery/description.rs`. The demo library makes no calls.
- A new state the window can show gets a scene in `src/ui/demo.rs` first. Each scene's element tree is a golden in `goldens/`.
- A press whose result needs a real call gets a test in `src/ui/tests/cloud.rs`: `Gallery::with_library` opens the view on a library connected to the fake API from `tests/support/`, so the press runs its call and the answer lands in the test.
- The benchmark contract (`src/ui/benchmark.rs`, `benchmark.py`) is a gate other runs compare against. Change its keys or definitions only on purpose, and update the README's measurement section with them.

## Working here

1. While you work, run `cargo test --features desktop <filter>`. Before you hand off, run `scripts/check`.
2. Render each scene your change shows (`--snap out.png --scene <name> --at 1.5`) and read every image. Rewrite goldens only for a change you meant (`PHOTO_PROTOTYPE_UPDATE_GOLDENS=1 cargo test --features desktop goldens`), read the diff, and commit them with the change.
3. Time-driven behaviour also needs a real window: run the GUI benchmark from the README with an unlocked, awake display.
