//! Data-first corpus of Tass ATProto lexicon documents.
//!
//! This crate intentionally exposes lexicons as JSON text, not Rust bindings.
//! Generated bindings, validators, docs, and sample checks should treat this
//! corpus as their source input. The `samples` feature is enabled by default and
//! embeds generated example records; disable default features to exclude them.

/// One lexicon document embedded in the corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconDoc {
    /// Lexicon NSID.
    pub nsid: &'static str,
    /// File name inside [`LEXICON_DIR`].
    pub file_name: &'static str,
    /// Canonical JSON text.
    pub json: &'static str,
}

/// One example record embedded in the corpus.
#[cfg(feature = "samples")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRecord {
    /// File name inside [`SAMPLE_DIR`].
    pub file_name: &'static str,
    /// Human-readable description of the example.
    pub description: &'static str,
    /// Canonical JSON text.
    pub json: &'static str,
}

/// Crate-local directory containing canonical lexicon JSON files.
pub const LEXICON_DIR: &str = "crates/tass-lex-corpus/lexicons";

/// Crate-local directory containing canonical example record JSON files.
#[cfg(feature = "samples")]
pub const SAMPLE_DIR: &str = "crates/tass-lex-corpus/samples";

/// Embedded `com.superbfowle.tass.*` lexicon documents.
pub const LEXICONS: &[LexiconDoc] = &[
    LexiconDoc {
        nsid: "com.superbfowle.tass.enervate",
        file_name: "com.superbfowle.tass.enervate.json",
        json: include_str!("../lexicons/com.superbfowle.tass.enervate.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.form",
        file_name: "com.superbfowle.tass.form.json",
        json: include_str!("../lexicons/com.superbfowle.tass.form.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.meditate",
        file_name: "com.superbfowle.tass.meditate.json",
        json: include_str!("../lexicons/com.superbfowle.tass.meditate.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.node",
        file_name: "com.superbfowle.tass.node.json",
        json: include_str!("../lexicons/com.superbfowle.tass.node.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.resonance",
        file_name: "com.superbfowle.tass.resonance.json",
        json: include_str!("../lexicons/com.superbfowle.tass.resonance.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.tassilize",
        file_name: "com.superbfowle.tass.tassilize.json",
        json: include_str!("../lexicons/com.superbfowle.tass.tassilize.json"),
    },
];

/// Embedded example records generated from the current Tass builders.
#[cfg(feature = "samples")]
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

/// Iterate all embedded lexicon documents in deterministic NSID order.
pub fn iter() -> impl Iterator<Item = LexiconDoc> {
    LEXICONS.iter().copied()
}

/// Iterate all embedded example records in deterministic file-name order.
#[cfg(feature = "samples")]
pub fn iter_samples() -> impl Iterator<Item = SampleRecord> {
    SAMPLES.iter().copied()
}

/// Return one embedded lexicon by NSID.
pub fn get(nsid: &str) -> Option<LexiconDoc> {
    LEXICONS.iter().copied().find(|doc| doc.nsid == nsid)
}

/// Return one embedded example record by file name.
#[cfg(feature = "samples")]
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
    fn corpus_contains_tass_lexicons() {
        assert_eq!(LEXICONS.len(), 6);
        assert_eq!(LEXICON_DIR, "crates/tass-lex-corpus/lexicons");
        assert!(get("com.superbfowle.tass.node").is_some());
        assert!(iter().all(|doc| doc.file_name.ends_with(".json")));
        assert!(iter().all(|doc| doc.json.contains(doc.nsid)));
    }

    #[cfg(feature = "samples")]
    #[test]
    fn corpus_contains_tass_samples() {
        assert_eq!(SAMPLES.len(), 4);
        assert_eq!(SAMPLE_DIR, "crates/tass-lex-corpus/samples");
        assert!(get_sample("node-crystal-spring.example.json").is_some());
        assert!(iter_samples().all(|sample| sample.file_name.ends_with(".example.json")));
        assert!(iter_samples().all(|sample| sample.json.contains("createdAt")));
    }
}
