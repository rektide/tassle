//! Example Tass records.
//!
//! A committed snapshot of example `at.telluri.*` records, regenerated
//! from the generated builders by `cargo xtask samples`. The canonical lexicon
//! schemas live in `tass-lex-schema`; this crate only ships fixture records.

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
pub const SAMPLE_DIR: &str = "crates/tass-lex-sample/samples";

/// Embedded example records generated from the current Tass builders.
pub const SAMPLES: &[SampleRecord] = &[
    SampleRecord {
        file_name: "enervate-spend.example.json",
        description: "Spending 2q from the silver-coin tass to fuel a coincidence.",
        json: include_str!("../samples/enervate-spend.example.json"),
    },
    SampleRecord {
        file_name: "meditate-dawn-pull.example.json",
        description: "Meditating at the Crystal Spring, drawing 3q into the mage's pattern.",
        json: include_str!("../samples/meditate-dawn-pull.example.json"),
    },
    SampleRecord {
        file_name: "node-crystal-spring.example.json",
        description: "A rating-3 Node with dynamic resonance and a default ambient pool (15q).",
        json: include_str!("../samples/node-crystal-spring.example.json"),
    },
    SampleRecord {
        file_name: "tassilize-silver-coin.example.json",
        description: "Genesis record: 5q crystallized at the Crystal Spring node as a silver coin.",
        json: include_str!("../samples/tassilize-silver-coin.example.json"),
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
    fn sample_crate_contains_tass_samples() {
        assert_eq!(SAMPLES.len(), 4);
        assert_eq!(SAMPLE_DIR, "crates/tass-lex-sample/samples");
        assert!(get_sample("node-crystal-spring.example.json").is_some());
        assert!(iter_samples().all(|sample| sample.file_name.ends_with(".example.json")));
        assert!(iter_samples().all(|sample| sample.json.contains("createdAt")));
    }
}
