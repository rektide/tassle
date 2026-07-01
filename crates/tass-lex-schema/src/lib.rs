//! Canonical Tass ATProto lexicon JSON documents.
//!
//! This crate intentionally exposes lexicons as JSON text, not Rust bindings.
//! Generated bindings (`tass-lexicons`), validators, docs, and the xtask
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

/// Embedded `at.telluri.*` lexicon documents.
pub const LEXICONS: &[LexiconDoc] = &[
    LexiconDoc {
        nsid: "at.telluri.act.enervate",
        file_name: "at.telluri.act.enervate.json",
        json: include_str!("../lexicons/at.telluri.act.enervate.json"),
    },
    LexiconDoc {
        nsid: "at.telluri.act.meditate",
        file_name: "at.telluri.act.meditate.json",
        json: include_str!("../lexicons/at.telluri.act.meditate.json"),
    },
    LexiconDoc {
        nsid: "at.telluri.act.tassilize",
        file_name: "at.telluri.act.tassilize.json",
        json: include_str!("../lexicons/at.telluri.act.tassilize.json"),
    },
    LexiconDoc {
        nsid: "at.telluri.node",
        file_name: "at.telluri.node.json",
        json: include_str!("../lexicons/at.telluri.node.json"),
    },
    LexiconDoc {
        nsid: "at.telluri.resonance",
        file_name: "at.telluri.resonance.json",
        json: include_str!("../lexicons/at.telluri.resonance.json"),
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
        assert_eq!(LEXICONS.len(), 5);
        assert_eq!(LEXICON_DIR, "crates/tass-lex-schema/lexicons");
        assert!(get("at.telluri.node").is_some());
        assert!(iter().all(|doc| doc.file_name.ends_with(".json")));
        assert!(iter().all(|doc| doc.json.contains(doc.nsid)));
    }
}
