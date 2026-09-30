//! Interface text for counts and sizes.

use crate::ImportSummary;

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

#[cfg(test)]
mod tests {
    use super::*;

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
