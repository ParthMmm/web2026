use anyhow::{Context, Result};
use photo_prototype::{Library, LibraryOptions};
use std::{path::PathBuf, time::Duration};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let usage = "Usage: export-film-metadata <library-directory> <destination.json>";
    let root = PathBuf::from(args.next().context(usage)?);
    let destination = PathBuf::from(args.next().context(usage)?);
    anyhow::ensure!(args.next().is_none(), "{usage}");
    let library = Library::open(LibraryOptions::new(root))?;
    while library.film_metadata_busy() {
        library
            .recv_timeout(Duration::from_secs(120))
            .context("Film metadata worker stalled")?;
    }
    library.export_film_metadata(&destination)?;
    println!(
        "Exported {} public photo records",
        library.current_photos()?.len()
    );
    Ok(())
}
