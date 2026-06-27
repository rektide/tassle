//! Data-first corpus of Tass ATProto lexicon documents.
//!
//! This crate intentionally exposes lexicons as JSON text, not Rust bindings.
//! Generated bindings, validators, docs, and samples should treat this corpus as
//! their source input.

/// One lexicon document embedded in the corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconDoc {
    /// Lexicon NSID.
    pub nsid: &'static str,
    /// Canonical JSON text.
    pub json: &'static str,
}

/// Embedded `com.superbfowle.tass.*` lexicon documents.
pub const LEXICONS: &[LexiconDoc] = &[
    LexiconDoc {
        nsid: "com.superbfowle.tass.enervate",
        json: include_str!("../../../lexicons/com.superbfowle.tass.enervate.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.form",
        json: include_str!("../../../lexicons/com.superbfowle.tass.form.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.meditate",
        json: include_str!("../../../lexicons/com.superbfowle.tass.meditate.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.node",
        json: include_str!("../../../lexicons/com.superbfowle.tass.node.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.resonance",
        json: include_str!("../../../lexicons/com.superbfowle.tass.resonance.json"),
    },
    LexiconDoc {
        nsid: "com.superbfowle.tass.tassilize",
        json: include_str!("../../../lexicons/com.superbfowle.tass.tassilize.json"),
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
    fn corpus_contains_tass_lexicons() {
        assert_eq!(LEXICONS.len(), 6);
        assert!(get("com.superbfowle.tass.node").is_some());
        assert!(iter().all(|doc| doc.json.contains(doc.nsid)));
    }
}
