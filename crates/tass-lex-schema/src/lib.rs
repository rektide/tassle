//! Canonical Tass ATProto lexicon JSON documents.
//!
//! This crate intentionally exposes lexicons as JSON text, not Rust bindings.
//! Generated bindings (`tassle-lexicons`), validators, docs, and the xtask
//! sample generator treat this crate as their source input.

/// One lexicon document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconDoc {
    /// Lexicon NSID.
    pub nsid: &'static str,
    /// File name inside [`LEXICON_DIR`].
    pub file_name: &'static str,
    /// Canonical JSON text.
    pub json: &'static str,
}

/// Crate-local directory containing canonical lexicon JSON files.
pub const LEXICON_DIR: &str = "crates/tass-lex-schema/lexicons";

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

/// Iterate all embedded lexicon documents in deterministic NSID order.
pub fn iter() -> impl Iterator<Item = LexiconDoc> {
    LEXICONS.iter().copied()
}

/// Return one embedded lexicon by NSID.
pub fn get(nsid: &str) -> Option<LexiconDoc> {
    LEXICONS.iter().copied().find(|doc| doc.nsid == nsid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_tass_lexicons() {
        assert_eq!(LEXICONS.len(), 6);
        assert_eq!(LEXICON_DIR, "crates/tass-lex-schema/lexicons");
        assert!(get("com.superbfowle.tass.node").is_some());
        assert!(iter().all(|doc| doc.file_name.ends_with(".json")));
        assert!(iter().all(|doc| doc.json.contains(doc.nsid)));
    }
}
