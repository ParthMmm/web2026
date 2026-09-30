//! Allowlisted photographic metadata read from an unchanged original JPEG.
//!
//! Only shooting details are extracted. GPS, serial numbers, and arbitrary
//! private tags are never read into the catalog.

use exif::{Exif, In, Reader, Tag, Value};
use std::{fmt, fs::File, io::BufReader, path::Path};

/// Exposure time as the camera recorded it, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExposureTime {
    numerator: u32,
    denominator: u32,
}

impl ExposureTime {
    pub fn new(numerator: u32, denominator: u32) -> Option<Self> {
        (numerator > 0 && denominator > 0).then_some(Self {
            numerator,
            denominator,
        })
    }

    pub fn numerator(self) -> u32 {
        self.numerator
    }

    pub fn denominator(self) -> u32 {
        self.denominator
    }

    pub fn seconds(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
}

/// Long exposures read as seconds; short ones as the familiar reciprocal.
const LONG_EXPOSURE_SECONDS: f64 = 0.3;

impl fmt::Display for ExposureTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = self.seconds();
        if seconds >= LONG_EXPOSURE_SECONDS {
            write!(f, "{} s", trim_decimal(seconds, 1))
        } else if self.numerator == 1 {
            write!(f, "1/{} s", self.denominator)
        } else {
            write!(f, "1/{} s", (1.0 / seconds).round())
        }
    }
}

/// Capture time in the camera's local clock. EXIF has no time zone unless the
/// optional offset tag is present; none is invented here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureTime {
    local: String,
    offset: Option<String>,
}

impl CaptureTime {
    /// `local` is `YYYY-MM-DDTHH:MM:SS`; `offset` is `+HH:MM` or `-HH:MM`.
    pub fn new(local: impl Into<String>, offset: Option<String>) -> Option<Self> {
        let local = local.into();
        let valid_local = parse_local_time(&local.replacen('T', " ", 1).replace('-', ":"))
            .is_some_and(|parsed| parsed == local);
        let valid_offset = offset.as_deref().is_none_or(is_valid_offset);
        (valid_local && valid_offset).then_some(Self { local, offset })
    }

    pub fn local(&self) -> &str {
        &self.local
    }

    pub fn offset(&self) -> Option<&str> {
        self.offset.as_deref()
    }
}

impl fmt::Display for CaptureTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (date, time) = self.local.split_once('T').unwrap_or((&self.local, ""));
        write!(f, "{date} {}", &time[..time.len().min(5)])?;
        if let Some(offset) = &self.offset {
            write!(f, " (UTC{offset})")?;
        }
        Ok(())
    }
}

/// Typed shooting details. Every field is optional: Lightroom only writes what
/// the export preset includes, and absent values stay absent.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct PhotoMetadata {
    pub iso: Option<u32>,
    pub exposure_time: Option<ExposureTime>,
    pub f_number: Option<f64>,
    pub focal_length_mm: Option<f64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub captured_at: Option<CaptureTime>,
}

impl PhotoMetadata {
    /// For example `ISO 400 · 1/250 s · f/2.8 · 85 mm`, omitting absent parts.
    pub fn exposure_summary(&self) -> Option<String> {
        let parts: Vec<String> = [
            self.iso.map(|iso| format!("ISO {iso}")),
            self.exposure_time.map(|time| time.to_string()),
            self.f_number
                .map(|f_number| format!("f/{}", trim_decimal(f_number, 1))),
            self.focal_length_mm.map(|focal| {
                let places = if focal < 10.0 { 1 } else { 0 };
                format!("{} mm", trim_decimal(focal, places))
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// Camera name without repeating a make the model already contains.
    pub fn camera(&self) -> Option<String> {
        match (&self.camera_make, &self.camera_model) {
            (Some(make), Some(model)) if model.to_lowercase().starts_with(&make.to_lowercase()) => {
                Some(model.clone())
            }
            (Some(make), Some(model)) => Some(format!("{make} {model}")),
            (None, Some(model)) => Some(model.clone()),
            (Some(make), None) => Some(make.clone()),
            (None, None) => None,
        }
    }
}

/// What import needs from the original's EXIF block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceExif {
    pub metadata: PhotoMetadata,
    /// EXIF orientation, 1–8. Missing or malformed values read as 1 (upright).
    pub orientation: u16,
}

impl SourceExif {
    /// Whether displaying upright swaps the stored width and height.
    pub fn swaps_dimensions(&self) -> bool {
        (5..=8).contains(&self.orientation)
    }
}

/// Missing or malformed EXIF yields empty metadata; it never fails an import.
pub fn read_source_exif(path: &Path) -> SourceExif {
    let Some(exif) = File::open(path).ok().and_then(|file| {
        Reader::new()
            .read_from_container(&mut BufReader::new(file))
            .ok()
    }) else {
        return SourceExif {
            orientation: 1,
            ..SourceExif::default()
        };
    };
    SourceExif {
        metadata: PhotoMetadata {
            iso: uint(&exif, Tag::PhotographicSensitivity).filter(|iso| *iso > 0),
            exposure_time: rational(&exif, Tag::ExposureTime)
                .and_then(|(numerator, denominator)| ExposureTime::new(numerator, denominator)),
            f_number: positive_rational(&exif, Tag::FNumber),
            focal_length_mm: positive_rational(&exif, Tag::FocalLength),
            camera_make: ascii(&exif, Tag::Make),
            camera_model: ascii(&exif, Tag::Model),
            lens: ascii(&exif, Tag::LensModel),
            captured_at: ascii(&exif, Tag::DateTimeOriginal)
                .and_then(|value| parse_local_time(&value))
                .and_then(|local| {
                    let offset =
                        ascii(&exif, Tag::OffsetTimeOriginal).filter(|o| is_valid_offset(o));
                    CaptureTime::new(local, offset)
                }),
        },
        orientation: uint(&exif, Tag::Orientation)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| (1..=8).contains(value))
            .unwrap_or(1),
    }
}

fn uint(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

fn rational(exif: &Exif, tag: Tag) -> Option<(u32, u32)> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(values) => values.first().map(|value| (value.num, value.denom)),
        _ => None,
    }
}

fn positive_rational(exif: &Exif, tag: Tag) -> Option<f64> {
    let (numerator, denominator) = rational(exif, tag)?;
    let value = f64::from(numerator) / f64::from(denominator);
    (value.is_finite() && value > 0.0).then_some(value)
}

fn ascii(exif: &Exif, tag: Tag) -> Option<String> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(values) => {
            let text = String::from_utf8(values.first()?.clone()).ok()?;
            let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            (!text.is_empty()).then(|| text.to_owned())
        }
        _ => None,
    }
}

/// EXIF `YYYY:MM:DD HH:MM:SS` to `YYYY-MM-DDTHH:MM:SS`; rejects impossible values.
fn parse_local_time(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 19
        && bytes.iter().enumerate().all(|(ix, byte)| match ix {
            4 | 7 | 13 | 16 => *byte == b':',
            10 => *byte == b' ',
            _ => byte.is_ascii_digit(),
        });
    if !shape_ok {
        return None;
    }
    let number = |range: std::ops::Range<usize>| value[range].parse::<u32>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let in_range = year >= 1
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 61;
    in_range.then(|| format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"))
}

fn is_valid_offset(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 6
        && matches!(bytes[0], b'+' | b'-')
        && bytes[3] == b':'
        && [1, 2, 4, 5].iter().all(|ix| bytes[*ix].is_ascii_digit())
        && value[1..3].parse::<u32>().is_ok_and(|hours| hours <= 14)
        && value[4..6].parse::<u32>().is_ok_and(|minutes| minutes < 60)
}

fn trim_decimal(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> PhotoMetadata {
        PhotoMetadata {
            iso: Some(400),
            exposure_time: ExposureTime::new(1, 250),
            f_number: Some(2.8),
            focal_length_mm: Some(85.0),
            ..PhotoMetadata::default()
        }
    }

    #[test]
    fn summarizes_complete_exposure_settings() {
        assert_eq!(
            metadata().exposure_summary().as_deref(),
            Some("ISO 400 · 1/250 s · f/2.8 · 85 mm")
        );
    }

    #[test]
    fn omits_absent_parts_without_placeholders() {
        let partial = PhotoMetadata {
            f_number: Some(8.0),
            ..PhotoMetadata::default()
        };
        assert_eq!(partial.exposure_summary().as_deref(), Some("f/8"));
        assert_eq!(PhotoMetadata::default().exposure_summary(), None);
    }

    #[test]
    fn formats_long_and_non_unit_exposures() {
        assert_eq!(ExposureTime::new(30, 1).unwrap().to_string(), "30 s");
        assert_eq!(ExposureTime::new(13, 10).unwrap().to_string(), "1.3 s");
        assert_eq!(ExposureTime::new(1, 2).unwrap().to_string(), "0.5 s");
        assert_eq!(ExposureTime::new(10, 2500).unwrap().to_string(), "1/250 s");
        assert_eq!(ExposureTime::new(0, 250), None);
    }

    #[test]
    fn keeps_phone_focal_lengths_precise() {
        let phone = PhotoMetadata {
            focal_length_mm: Some(6.86),
            ..PhotoMetadata::default()
        };
        assert_eq!(phone.exposure_summary().as_deref(), Some("6.9 mm"));
    }

    #[test]
    fn does_not_repeat_the_make_in_the_camera_name() {
        let mut camera = PhotoMetadata {
            camera_make: Some("Canon".into()),
            camera_model: Some("Canon EOS R5".into()),
            ..PhotoMetadata::default()
        };
        assert_eq!(camera.camera().as_deref(), Some("Canon EOS R5"));
        camera.camera_make = Some("FUJIFILM".into());
        camera.camera_model = Some("X-T5".into());
        assert_eq!(camera.camera().as_deref(), Some("FUJIFILM X-T5"));
    }

    #[test]
    fn rejects_malformed_capture_times() {
        assert_eq!(
            parse_local_time("2025:09:12 18:04:31").as_deref(),
            Some("2025-09-12T18:04:31")
        );
        for invalid in [
            "0000:00:00 00:00:00",
            "2025:13:01 00:00:00",
            "    :  :     :  :  ",
            "2025-09-12",
        ] {
            assert_eq!(parse_local_time(invalid), None, "{invalid}");
        }
        assert!(CaptureTime::new("2025-09-12T18:04:31", Some("+02:00".into())).is_some());
        assert!(CaptureTime::new("2025-09-12T18:04:31", Some("02:00".into())).is_none());
    }

    #[test]
    fn displays_capture_time_without_inventing_a_zone() {
        let local = CaptureTime::new("2025-09-12T18:04:31", None).unwrap();
        assert_eq!(local.to_string(), "2025-09-12 18:04");
        let zoned = CaptureTime::new("2025-09-12T18:04:31", Some("-07:00".into())).unwrap();
        assert_eq!(zoned.to_string(), "2025-09-12 18:04 (UTC-07:00)");
    }
}
