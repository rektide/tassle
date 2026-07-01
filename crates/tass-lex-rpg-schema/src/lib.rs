//! Raw upstream rpg.actor / equipment.rpg lexicon JSON documents.
//!
//! This crate mirrors [`tass-lex-schema`] for the *external* rpg schemas tass
//! depends on. It is the source input for [`tass-lex-rpg`] (generated bindings).
//!
//! ## Three-layer design
//!
//! 1. **Raw** — the upstream rpg JSON snapshots, patched for Jacquard codegen
//!    compatibility (bare `"type":"object"` → add `"properties":{}`;
//!    `"format":"date"` → `"format":"datetime"`). Patches are minimal; see the
//!    patch script committed alongside the lexicons.
//! 2. **Overlay** — mage-specific schema additions layered on top of the raw
//!    upstream set. Starts empty; fills in via the `tass-lex-mage-codegen`
//!    follow-up. Lives in `overlay/`.
//! 3. **Combined** — raw + overlay merged. This is the main export: the view
//!    codegen consumes and the set consumers should treat as "the rpg schemas
//!    as tass sees them." While the overlay is empty, combined ≡ raw.
//!
//! [`tass-lex-schema`]: ../../tass-lex-schema
//! [`tass-lex-rpg`]: ../../tass-lex-rpg

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

/// Crate-local directory containing raw upstream rpg lexicon JSON files.
pub const LEXICON_DIR: &str = "crates/tass-lex-rpg-schema/lexicons";

/// Crate-local directory containing mage overlay lexicon additions (empty for now).
pub const OVERLAY_DIR: &str = "crates/tass-lex-rpg-schema/overlay";

/// Embedded raw upstream rpg lexicon documents (patched for codegen compat).
pub const RAW_LEXICONS: &[LexiconDoc] = &[
    LexiconDoc {
        nsid: "actor.rpg.master",
        file_name: "actor.rpg.master.json",
        json: include_str!("../lexicons/actor.rpg.master.json"),
    },
    LexiconDoc {
        nsid: "actor.rpg.sprite",
        file_name: "actor.rpg.sprite.json",
        json: include_str!("../lexicons/actor.rpg.sprite.json"),
    },
    LexiconDoc {
        nsid: "actor.rpg.stats",
        file_name: "actor.rpg.stats.json",
        json: include_str!("../lexicons/actor.rpg.stats.json"),
    },
    LexiconDoc {
        nsid: "equipment.rpg.give",
        file_name: "equipment.rpg.give.json",
        json: include_str!("../lexicons/equipment.rpg.give.json"),
    },
    LexiconDoc {
        nsid: "equipment.rpg.item",
        file_name: "equipment.rpg.item.json",
        json: include_str!("../lexicons/equipment.rpg.item.json"),
    },
];

/// Embedded mage overlay lexicon documents (empty — content fills in later).
pub const OVERLAY_LEXICONS: &[LexiconDoc] = &[];

/// Combined view: raw upstream + mage overlay. This is what codegen consumes
/// and the set tass treats as "the rpg schemas." While the overlay is empty,
/// this is identical to [`RAW_LEXICONS`].
pub fn iter() -> impl Iterator<Item = LexiconDoc> {
    RAW_LEXICONS
        .iter()
        .chain(OVERLAY_LEXICONS.iter())
        .copied()
}

/// Iterate raw upstream documents only.
pub fn iter_raw() -> impl Iterator<Item = LexiconDoc> {
    RAW_LEXICONS.iter().copied()
}

/// Iterate mage overlay documents only.
pub fn iter_overlay() -> impl Iterator<Item = LexiconDoc> {
    OVERLAY_LEXICONS.iter().copied()
}

/// Return one document from the combined view by NSID.
pub fn get(nsid: &str) -> Option<LexiconDoc> {
    iter().find(|doc| doc.nsid == nsid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_contains_rpg_lexicons() {
        assert_eq!(RAW_LEXICONS.len(), 5);
        assert!(get("actor.rpg.stats").is_some());
        assert!(get("equipment.rpg.item").is_some());
        assert!(iter().all(|doc| doc.file_name.ends_with(".json")));
        assert!(iter().all(|doc| doc.json.contains(doc.nsid)));
    }

    #[test]
    fn combined_is_raw_plus_overlay() {
        assert_eq!(iter().count(), RAW_LEXICONS.len() + OVERLAY_LEXICONS.len());
    }
}
