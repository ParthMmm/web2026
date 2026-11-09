use std::path::PathBuf;

const LIBRARY_DIRECTORY: &str = "Library/Application Support/dev.parth.photo-prototype/library";

fn library_root() -> PathBuf {
    std::env::var_os("PHOTO_LIBRARY_DIR").map_or_else(
        || {
            PathBuf::from(std::env::var_os("HOME").expect("HOME is required"))
                .join(LIBRARY_DIRECTORY)
        },
        PathBuf::from,
    )
}

fn main() {
    let inputs = photo_prototype::ui::inputs();
    photo_prototype::ui::run(library_root(), inputs);
}
