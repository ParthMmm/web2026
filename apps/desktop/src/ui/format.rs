//! Interface text for counts and sizes.

use super::gallery::UploadRun;
use crate::{
    ImportSummary,
    cloud::{CloudState, CloudStep, ObjectKind},
};

const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
const DECIMAL_UNIT: f64 = 1000.0;

/// Decimal file sizes, as Finder shows them.
pub(super) fn bytes(count: u64) -> String {
    if count < 1000 {
        return format!("{count} bytes");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = count as f64 / DECIMAL_UNIT;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < DECIMAL_UNIT {
            break;
        }
        value /= DECIMAL_UNIT;
        unit = next;
    }
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

/// Groups thousands: `12,345`.
pub(super) fn number(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// `1 photo`, `2,048 photos`.
pub(super) fn count(value: usize, singular: &str, plural: &str) -> String {
    let noun = if value == 1 { singular } else { plural };
    format!("{} {noun}", number(value))
}

/// The library's total weight, which the grid cannot show. A status line that
/// repeated the photo count would only restate the title strip.
pub(super) fn library_size(total_bytes: u64) -> String {
    format!("{} of originals", bytes(total_bytes))
}

/// One sentence describing a finished upload run.
pub(super) fn upload_summary(run: &UploadRun) -> String {
    let mut summary = if run.drafts == 1 {
        "Uploaded 1 photo as a private draft".to_owned()
    } else if run.drafts > 1 {
        format!("Uploaded {} photos as private drafts", number(run.drafts))
    } else {
        "Nothing uploaded".to_owned()
    };
    if run.failed > 0 {
        summary.push_str(&format!(" · {} failed", number(run.failed)));
    }
    summary
}

/// A cloud state in a few words, for the inspector and for a tile's label.
pub(super) fn cloud_state(state: &CloudState) -> String {
    match state {
        CloudState::Local => "Only in this library".into(),
        CloudState::Queued => "Waiting to upload".into(),
        CloudState::Uploading { missing, .. } => {
            let sent = ObjectKind::ALL.len() - missing.len();
            format!("Uploading · {sent} of {} files", ObjectKind::ALL.len())
        }
        CloudState::Draft { .. } => "Private draft".into(),
        CloudState::Publishing { .. } => "Publishing…".into(),
        CloudState::Published { .. } => "Published".into(),
        CloudState::Unpublishing { .. } => "Unpublishing…".into(),
        CloudState::Failed { step, .. } => match step {
            CloudStep::Upload => "Upload failed",
            CloudStep::Publish => "Publish failed",
            CloudStep::Unpublish => "Unpublish failed",
        }
        .into(),
    }
}

/// What stopped, as the start of a sentence.
pub(super) fn failed_step(step: CloudStep) -> &'static str {
    match step {
        CloudStep::Upload => "Couldn't upload",
        CloudStep::Publish => "Couldn't publish",
        CloudStep::Unpublish => "Couldn't unpublish",
    }
}

/// What the status bar says when a publish or unpublish of `name` ends, if anything.
pub(super) fn publish_outcome(
    before: &CloudState,
    after: &CloudState,
    name: &str,
) -> Option<String> {
    Some(match (before, after) {
        (CloudState::Publishing { .. }, CloudState::Published { .. }) => {
            format!("Published {name}")
        }
        (CloudState::Unpublishing { .. }, CloudState::Draft { .. }) => {
            format!("Unpublished {name}. It is a private draft again")
        }
        (
            CloudState::Publishing { .. } | CloudState::Unpublishing { .. },
            CloudState::Failed { step, .. },
        ) => format!("{} {name}", failed_step(*step)),
        _ => return None,
    })
}

/// The status bar's words while photos are being published or unpublished.
pub(super) fn publishing_status(
    publishing: usize,
    unpublishing: usize,
    connected: bool,
) -> Option<String> {
    let mut parts = Vec::new();
    if publishing > 0 {
        parts.push(format!(
            "Publishing {}…",
            count(publishing, "photo", "photos")
        ));
    }
    if unpublishing > 0 {
        parts.push(format!(
            "Unpublishing {}…",
            count(unpublishing, "photo", "photos")
        ));
    }
    if parts.is_empty() {
        return None;
    }
    if !connected {
        parts.push("not connected".into());
    }
    Some(parts.join(" · "))
}

/// One sentence describing a finished import run.
pub(super) fn import_summary(summary: &ImportSummary) -> String {
    let already = summary.duplicates + summary.unchanged;
    let mut parts = Vec::new();
    if summary.added > 0 {
        parts.push(format!("Added {}", count(summary.added, "photo", "photos")));
    }
    if summary.relinked > 0 {
        parts.push(format!(
            "reconnected {}",
            count(summary.relinked, "original", "originals")
        ));
    }
    if already > 0 {
        parts.push(format!("{} already in the library", number(already)));
    }
    if summary.failed > 0 {
        parts.push(format!("{} couldn't be imported", number(summary.failed)));
    }
    let Some(first) = parts.first_mut() else {
        return "No JPEGs found".into();
    };
    if let Some(initial) = first.get(..1) {
        let upper = initial.to_uppercase();
        first.replace_range(..1, &upper);
    }
    parts.join(" · ")
}

/// Photos assets the picker selected but could not hand over, with the first reason.
pub(super) fn photos_not_loaded(failed: usize, reason: &str) -> String {
    format!(
        "{} from Photos couldn't be loaded ({reason})",
        count(failed, "photo", "photos")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_that_did_not_load_are_counted_with_the_first_reason() {
        assert_eq!(
            photos_not_loaded(2, "The network connection was lost."),
            "2 photos from Photos couldn't be loaded (The network connection was lost.)"
        );
    }

    #[test]
    fn import_summaries_name_each_outcome_once() {
        let summary = ImportSummary {
            added: 12,
            duplicates: 2,
            unchanged: 1,
            failed: 1,
            ..ImportSummary::default()
        };
        assert_eq!(
            import_summary(&summary),
            "Added 12 photos · 3 already in the library · 1 couldn't be imported"
        );
        let relinked = ImportSummary {
            relinked: 1,
            ..ImportSummary::default()
        };
        assert_eq!(import_summary(&relinked), "Reconnected 1 original");
        assert_eq!(import_summary(&ImportSummary::default()), "No JPEGs found");
    }

    #[test]
    fn sizes_use_decimal_units_with_useful_precision() {
        assert_eq!(bytes(512), "512 bytes");
        assert_eq!(bytes(1_000), "1.0 KB");
        assert_eq!(bytes(3_400_000), "3.4 MB");
        assert_eq!(bytes(48_000_000), "48 MB");
        assert_eq!(bytes(2_500_000_000), "2.5 GB");
    }

    #[test]
    fn counts_group_thousands_and_agree_in_number() {
        assert_eq!(count(1, "photo", "photos"), "1 photo");
        assert_eq!(count(0, "photo", "photos"), "0 photos");
        assert_eq!(count(1_234_567, "photo", "photos"), "1,234,567 photos");
        assert_eq!(number(999), "999");
    }
}
