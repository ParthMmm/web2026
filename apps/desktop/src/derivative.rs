//! Web derivatives rendered by the pinned libvips helper.

use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

pub const REQUIRED_VIPS: &str = "vips-8.18.3";

/// Bump when the encoder settings change so stale derivatives are regenerated.
pub const RECIPE_VERSION: u32 = 1;

pub const WEBP_QUALITY: u32 = 82;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DerivativeSize {
    /// Library grid and inspector placeholder.
    Grid,
    /// Phone-width public view.
    Mobile,
    /// Large preview and desktop public view.
    Desktop,
}

impl DerivativeSize {
    pub const ALL: [Self; 3] = [Self::Grid, Self::Mobile, Self::Desktop];

    /// Longest edge in pixels. Smaller originals are never upscaled.
    pub fn edge(self) -> u32 {
        match self {
            Self::Grid => 480,
            Self::Mobile => 1200,
            Self::Desktop => 2400,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Mobile => "mobile",
            Self::Desktop => "desktop",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.key() == key)
    }
}

/// Checks the helper version once per process before any image work.
pub fn require_vips() -> Result<()> {
    static VERIFIED: OnceLock<()> = OnceLock::new();
    if VERIFIED.get().is_some() {
        return Ok(());
    }
    let output = Command::new("vips")
        .arg("--version")
        .output()
        .context("libvips 8.18.3 is required; see apps/desktop/README.md")?;
    let version = String::from_utf8_lossy(&output.stdout);
    anyhow::ensure!(
        output.status.success() && version.trim() == REQUIRED_VIPS,
        "Unsupported libvips: expected {REQUIRED_VIPS}, found {}. See apps/desktop/README.md",
        version.trim()
    );
    let _ = VERIFIED.set(());
    Ok(())
}

/// Renders an upright, sRGB, metadata-free WebP no larger than `size.edge()`.
///
/// libvips applies EXIF orientation and converts an embedded ICC profile to
/// sRGB. Untagged sources are treated as sRGB. `keep=none` strips EXIF, XMP,
/// IPTC, and ICC from the output, which is correct because pixels are sRGB.
/// Truncated JPEGs fail instead of producing a partially grey image.
pub fn render(source: &Path, output: &Path, size: DerivativeSize) -> Result<(u32, u32)> {
    require_vips()?;
    let temporary = temporary_path(output);
    let edge = size.edge().to_string();
    let result = Command::new("vips")
        .env("VIPS_CONCURRENCY", "2")
        .arg("thumbnail")
        .arg(source)
        .arg(format!(
            "{}[Q={WEBP_QUALITY},keep=none]",
            temporary.display()
        ))
        .arg(&edge)
        .args(["--height", &edge, "--size", "down"])
        .args(["--output-profile", "srgb", "--fail-on", "truncated"])
        .output()
        .context("libvips is required; see apps/desktop/README.md")
        .and_then(|finished| {
            anyhow::ensure!(
                finished.status.success(),
                "{}",
                describe_vips_failure(&String::from_utf8_lossy(&finished.stderr))
            );
            let dimensions = image::image_dimensions(&temporary)
                .context("libvips produced an unreadable derivative")?;
            fs::rename(&temporary, output)?;
            Ok(dimensions)
        });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(output: &Path) -> PathBuf {
    // Two lanes can render the same derivative; each writes its own file and
    // the atomic rename makes the last identical result visible.
    let thread = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    output.with_extension(format!("{}-{thread}.tmp.webp", std::process::id()))
}

fn describe_vips_failure(stderr: &str) -> String {
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("unknown error");
    if detail.contains("not a known file format") || detail.contains("is not a known") {
        "Not a readable JPEG".to_owned()
    } else if detail.contains("truncated") || detail.contains("premature end") {
        format!("The JPEG is incomplete or damaged ({detail})")
    } else {
        format!("libvips couldn't convert this file ({detail})")
    }
}
