//! Example rpg records.
//!
//! A committed snapshot of example `actor.rpg.stats` / `equipment.rpg.item`
//! records. These are illustrative fixtures — not generated from the builders
//! (the rpg schemas are upstream, not our own). The canonical lexicon schemas
//! live in `tass-lex-rpg-schema`.

/// One example record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRecord {
    /// File name inside [`SAMPLE_DIR`].
    pub file_name: &'static str,
    /// Human-readable description of the example.
    pub description: &'static str,
    /// Canonical JSON text.
    pub json: &'static str,
}

/// Crate-local directory containing canonical example record JSON files.
pub const SAMPLE_DIR: &str = "crates/tass-lex-rpg-sample/samples";

/// Embedded example rpg records.
pub const SAMPLES: &[SampleRecord] = &[
    SampleRecord {
        file_name: "equipment-item.example.json",
        description: "A player's owned item, accepted from a provider's give record.",
        json: include_str!("../samples/equipment-item.example.json"),
    },
    SampleRecord {
        file_name: "mage-stats.example.json",
        description: "A Mage: the Ascension character sheet (actor.rpg.stats/mage).",
        json: include_str!("../samples/mage-stats.example.json"),
    },
];

/// Iterate all embedded example records in deterministic file-name order.
pub fn iter_samples() -> impl Iterator<Item = SampleRecord> {
    SAMPLES.iter().copied()
}

/// Return one embedded example record by file name.
pub fn get_sample(file_name: &str) -> Option<SampleRecord> {
    SAMPLES
        .iter()
        .copied()
        .find(|sample| sample.file_name == file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_crate_contains_rpg_samples() {
        assert_eq!(SAMPLES.len(), 2);
        assert_eq!(SAMPLE_DIR, "crates/tass-lex-rpg-sample/samples");
        assert!(get_sample("mage-stats.example.json").is_some());
        assert!(iter_samples().all(|sample| sample.file_name.ends_with(".example.json")));
        assert!(iter_samples().all(|sample| sample.json.contains("$type")));
    }
}
