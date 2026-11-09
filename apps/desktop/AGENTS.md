# AGENTS.md

A local macOS photo library, drawn with [Rune](https://github.com/anomalyco/rune) and rune-kit. It imports JPEGs into a SQLite catalog, makes renditions with libvips, and shows them in a grid, an inspector and a full-window viewer. It talks to no service. `README.md` describes every feature, the benchmark gate, and how to check the window; read the section for the part you change. Rune's own patterns are in the `rune-app` skill (`~/dotfiles/agents/skills/rune-app/`).

## Boundaries

- The person's library (`~/Library/Application Support/dev.parth.photo-prototype/library`) and their Photos library are off limits. Set `PHOTO_LIBRARY_DIR` to a scratch folder for any real-window run, and import only generated or authorized JPEGs.
- The app opens the library only on a plain launch, `--native`, or a benchmark run. Every other flag (`--snap`, `--tree`, `--script`, `--scene`) shows the demo library from `src/ui/demo.rs`: made-up photos, nothing on disk. Never commit a capture of a real library.

## Shape of the code

- `src/ui/gallery/` holds the window's state. Library events land in `events.rs`, and imports and pickers live in `import.rs`. `LibraryState::Demo` is the demo library; it has no `AppHandle` and starts no threads.
- `src/ui/view/` draws that state, one file per part (grid, inspector, overlays). The view reads no clock; `poll` refreshes anything time-driven, such as the performance line.
- A new state the window can show gets a scene in `src/ui/demo.rs` first. Each scene's element tree is a golden in `goldens/`.
- The benchmark contract (`src/ui/benchmark.rs`, `benchmark.py`) is a gate other runs compare against. Change its keys or definitions only on purpose, and update the README's measurement section with them.

## Working here

1. While you work, run `cargo test --features desktop <filter>`. Before you hand off, run `scripts/check`.
2. Render each scene your change shows (`--snap out.png --scene <name> --at 1.5`) and read every image. Rewrite goldens only for a change you meant (`PHOTO_PROTOTYPE_UPDATE_GOLDENS=1 cargo test --features desktop goldens`), read the diff, and commit them with the change.
3. Time-driven behaviour also needs a real window: run the GUI benchmark from the README with an unlocked, awake display.
