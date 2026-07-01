//! Pure `actor.rpg.stats/mage` domain — mage-block location + normalization.
//!
//! No jacquard, no IO: everything here takes a `serde_json::Value`/`Map` that
//! some transport layer already fetched and hands back a typed view. That keeps
//! the mage-sheet knowledge — where the mage block lives in the two production
//! record shapes, and how to read arete / willpower / quintessence / spheres
//! out of loose JSON — reusable by the CLI, the mage DAO (`tass-repo-mage`), a
//! web service, or the listener daemon alike.
//!
//! The pattern-quintessence value rules live in [`tass_quint`]; this crate only
//! *locates* the fields and defers to [`tass_quint::resolve`] for the value.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

/// Locate the mage field map inside an `actor.rpg.stats` record value.
///
/// Handles the two real production shapes: the per-system envelope
/// `{ system: "mage", data: {…} }` and the legacy inline `{ mage: {…} }`.
/// Returns `None` when neither shape carries a mage block.
pub fn mage_block(value: &Value) -> Option<&Map<String, Value>> {
    if value.get("system").and_then(Value::as_str) == Some("mage") {
        if let Some(obj) = value.get("data").and_then(Value::as_object) {
            return Some(obj);
        }
    }
    value.get("mage").and_then(Value::as_object)
}

/// Mutable counterpart of [`mage_block`], for write paths that patch the block
/// in place.
pub fn mage_block_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    let is_envelope = value.get("system").and_then(Value::as_str) == Some("mage");
    if is_envelope {
        return value.get_mut("data").and_then(Value::as_object_mut);
    }
    value.get_mut("mage").and_then(Value::as_object_mut)
}

/// A mage sheet read out of a record and normalized to canonical fields.
///
/// Serialized `camelCase` for the wire/output layers. `None` fields are absent
/// on the sheet; [`missing`](Self::missing) lists the canonical stats that had
/// no readable value, so a renderer can flag an incomplete sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedMageStats {
    pub arete: Option<i64>,
    pub willpower: Option<i64>,
    pub willpower_temporary: Option<i64>,
    /// Player-facing whole points — always the floor of the resolved `quint`
    /// millis (via [`tass_quint`]). Derived, not the raw sheet field.
    pub quintessence: Option<i64>,
    /// Raw Tassle extension field (`milliQuintessence`), in milli-quintessence.
    /// `None` when the sheet only carries the legacy `quintessence` integer.
    pub milli_quintessence: Option<i64>,
    pub paradox: Option<i64>,
    pub spheres: BTreeMap<String, i64>,
    /// Canonical stats with no readable value on this sheet.
    pub missing: Vec<String>,
}

/// Locate the mage block in `record` and normalize it. Returns `None` when the
/// record carries no mage block (see [`mage_block`]).
pub fn normalize(record: &Value) -> Option<NormalizedMageStats> {
    mage_block(record).map(normalize_block)
}

/// Normalize an already-located mage field map into [`NormalizedMageStats`].
///
/// Tolerant of the case/shape variance seen in real sheets: stat keys are read
/// through alias lists, and `willpower` may be a flat integer or a
/// `{ permanent, temporary }` object.
pub fn normalize_block(obj: &Map<String, Value>) -> NormalizedMageStats {
    let mut missing = Vec::new();
    let arete = number_field(obj, &["arete", "Arete"]);
    let willpower = willpower_field(obj);
    let willpower_temporary = willpower_temporary_field(obj);
    let quintessence_raw = number_field(obj, &["quintessence", "Quintessence"]);
    let milli_raw = number_field(obj, &["milliQuintessence", "MilliQuintessence"]);
    // milliQuintessence is the source of truth when the Tassle extension field
    // is present; otherwise hydrate from the legacy integer. quintessence
    // always shows the rounded-down points. See the tass-quint crate.
    let quintessence = tass_quint::resolve(milli_raw, quintessence_raw).map(|q| q.points());
    let paradox = number_field(obj, &["paradox", "Paradox"]);

    for (name, value) in [
        ("arete", arete),
        ("willpower", willpower),
        ("quintessence", quintessence),
        ("paradox", paradox),
    ] {
        if value.is_none() {
            missing.push(name.to_owned());
        }
    }

    let mut spheres = BTreeMap::new();
    for (canonical, aliases) in [
        ("correspondence", ["correspondence", "Correspondence", ""]),
        ("entropy", ["entropy", "Entropy", ""]),
        ("forces", ["forces", "Forces", "Force"]),
        ("life", ["life", "Life", ""]),
        ("matter", ["matter", "Matter", ""]),
        ("mind", ["mind", "Mind", ""]),
        ("prime", ["prime", "Prime", ""]),
        ("spirit", ["spirit", "Spirit", ""]),
        ("time", ["time", "Time", ""]),
    ] {
        let aliases = aliases
            .into_iter()
            .filter(|alias| !alias.is_empty())
            .collect::<Vec<_>>();
        if let Some(value) = number_field(obj, &aliases) {
            spheres.insert(canonical.to_owned(), value);
        } else {
            missing.push(canonical.to_owned());
        }
    }

    NormalizedMageStats {
        arete,
        willpower,
        willpower_temporary,
        quintessence,
        milli_quintessence: milli_raw,
        paradox,
        spheres,
        missing,
    }
}

/// First integer among `names` present on `obj`.
fn number_field(obj: &Map<String, Value>, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| obj.get(*name)?.as_i64())
}

/// Permanent willpower: a flat `willpower`/`Willpower` integer, else the
/// `willpower.permanent` sub-field.
fn willpower_field(obj: &Map<String, Value>) -> Option<i64> {
    number_field(obj, &["willpower", "Willpower"])
        .or_else(|| obj.get("willpower")?.as_object()?.get("permanent")?.as_i64())
}

/// Temporary willpower from the `willpower.temporary` sub-field, if present.
fn willpower_temporary_field(obj: &Map<String, Value>) -> Option<i64> {
    obj.get("willpower")?.as_object()?.get("temporary")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(fields: Value) -> Value {
        json!({ "system": "mage", "data": fields, "$type": "actor.rpg.stats" })
    }

    fn inline(fields: Value) -> Value {
        json!({ "mage": fields, "$type": "actor.rpg.stats" })
    }

    #[test]
    fn locates_block_in_both_shapes() {
        let f = json!({ "arete": 3 });
        assert_eq!(mage_block(&envelope(f.clone())).unwrap().get("arete"), Some(&json!(3)));
        assert_eq!(mage_block(&inline(f)).unwrap().get("arete"), Some(&json!(3)));
        assert!(mage_block(&json!({ "$type": "actor.rpg.stats" })).is_none());
    }

    #[test]
    fn normalize_returns_none_without_block() {
        assert!(normalize(&json!({ "$type": "actor.rpg.stats" })).is_none());
    }

    #[test]
    fn reads_core_stats_with_case_aliases() {
        let n = normalize(&inline(json!({
            "Arete": 4,
            "willpower": 6,
            "Paradox": 1,
        })))
        .unwrap();
        assert_eq!(n.arete, Some(4));
        assert_eq!(n.willpower, Some(6));
        assert_eq!(n.paradox, Some(1));
    }

    #[test]
    fn willpower_object_shape() {
        let n = normalize(&inline(json!({
            "willpower": { "permanent": 7, "temporary": 5 },
        })))
        .unwrap();
        assert_eq!(n.willpower, Some(7));
        assert_eq!(n.willpower_temporary, Some(5));
    }

    #[test]
    fn quintessence_prefers_milli_and_floors_points() {
        let n = normalize(&envelope(json!({ "milliQuintessence": 3500, "quintessence": 2 })))
            .unwrap();
        assert_eq!(n.milli_quintessence, Some(3500));
        assert_eq!(n.quintessence, Some(3)); // floor(3500/1000), not the stale legacy 2
    }

    #[test]
    fn quintessence_hydrates_from_legacy_integer() {
        let n = normalize(&inline(json!({ "quintessence": 5 }))).unwrap();
        assert_eq!(n.milli_quintessence, None);
        assert_eq!(n.quintessence, Some(5));
    }

    #[test]
    fn missing_lists_absent_stats() {
        let n = normalize(&inline(json!({ "arete": 3 }))).unwrap();
        assert!(n.missing.contains(&"willpower".to_owned()));
        assert!(n.missing.contains(&"forces".to_owned()));
        assert!(!n.missing.contains(&"arete".to_owned()));
    }
}
