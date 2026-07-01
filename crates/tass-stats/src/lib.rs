//! Pure `actor.rpg.stats` record-family shape detection + summaries.
//!
//! No jacquard, no IO: given a `serde_json::Value` a transport already fetched,
//! classify what *shape* of stats record it is (per-system envelope vs a legacy
//! inline system vs a self-aggregate) and summarize its systems/fields. This is
//! the general record-family knowledge — distinct from [`tass_mage`], which
//! interprets the mage system's *values*, and from `tass-repo`, which does
//! lexicon-agnostic access. The CLI's `mage` and `self` commands consume it; a
//! web service or listener inspecting stats records reuses it.
//!
//! [`tass_mage`]: https://docs.rs/tass-mage

use serde::Serialize;
use serde_json::{Map, Value};

/// A summary of one `actor.rpg.stats` record: its shape, which system it
/// carries, and the field names present. Serialized `camelCase` for output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub uri: String,
    pub cid: Option<String>,
    pub rkey: String,
    pub shape: String,
    pub system: Option<String>,
    pub fields: Vec<String>,
}

/// A summary of one system inside a self-aggregate record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSummary {
    pub key: String,
    pub kind: String,
    pub fields: Vec<String>,
}

/// The classification of a stats record's shape, produced by [`stats_payload`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPayload {
    /// The located system data, when the record carries an identifiable one.
    pub data: Option<Value>,
    /// A label for the record's shape: `per-system-envelope`,
    /// `legacy-inline-system`, `legacy-self-aggregate`, or `unknown`.
    pub shape: String,
    /// The system name, when known.
    pub system: Option<String>,
}

/// The last path segment of an AT-URI (its record key). (Kept local — a
/// one-liner not worth a dependency on the transport crate.)
fn rkey_from_uri(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

fn object<'a>(value: &'a Value, field: &str) -> Option<&'a Map<String, Value>> {
    value.get(field)?.as_object()
}

fn field_names(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

/// Classify an `actor.rpg.stats` record value: locate its system data and label
/// its shape. `rkey` disambiguates the legacy shapes (a `self` aggregate vs a
/// record whose top-level key names an inline system).
pub fn stats_payload(value: &Value, rkey: &str) -> StatsPayload {
    if let Some(system) = value.get("system").and_then(Value::as_str) {
        if let Some(data) = value.get("data") {
            return StatsPayload {
                data: Some(data.clone()),
                shape: "per-system-envelope".to_owned(),
                system: Some(system.to_owned()),
            };
        }
    }

    if rkey == "self" {
        return StatsPayload {
            data: None,
            shape: "legacy-self-aggregate".to_owned(),
            system: None,
        };
    }

    if let Some(system) = object(value, rkey) {
        return StatsPayload {
            data: Some(Value::Object(system.clone())),
            shape: "legacy-inline-system".to_owned(),
            system: Some(rkey.to_owned()),
        };
    }

    StatsPayload {
        data: None,
        shape: "unknown".to_owned(),
        system: None,
    }
}

/// Summarize a record by its AT-URI, CID, and value: derives the rkey, classifies
/// the shape, and lists the payload fields (falling back to the record's own
/// top-level fields when no system payload is found).
pub fn summarize_record(uri: &str, cid: Option<&str>, value: &Value) -> StatsSummary {
    let rkey = rkey_from_uri(uri).to_owned();
    let payload = stats_payload(value, &rkey);
    let summary_fields = payload
        .data
        .as_ref()
        .map(field_names)
        .unwrap_or_else(|| field_names(value));
    StatsSummary {
        uri: uri.to_owned(),
        cid: cid.map(ToOwned::to_owned),
        rkey,
        shape: payload.shape,
        system: payload.system,
        fields: summary_fields,
    }
}

/// Walk a self-aggregate record's top-level systems, skipping `$`-prefixed and
/// timestamp keys. Sorted by system key for stable output.
pub fn summarize_systems(raw: &Value) -> Vec<SystemSummary> {
    let Some(obj) = raw.as_object() else {
        return Vec::new();
    };
    let mut systems = Vec::new();
    for (key, value) in obj {
        if key.starts_with('$') || matches!(key.as_str(), "createdAt" | "updatedAt") {
            continue;
        }
        let kind = match value {
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::String(_) => "string",
            Value::Number(_) => "number",
            Value::Bool(_) => "bool",
            Value::Null => "null",
        };
        let fields = value
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        systems.push(SystemSummary {
            key: key.clone(),
            kind: kind.to_owned(),
            fields,
        });
    }
    systems.sort_by(|a, b| a.key.cmp(&b.key));
    systems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_per_system_envelope() {
        let v = json!({ "system": "mage", "data": { "arete": 3 } });
        let p = stats_payload(&v, "mage");
        assert_eq!(p.shape, "per-system-envelope");
        assert_eq!(p.system.as_deref(), Some("mage"));
        assert!(p.data.is_some());
    }

    #[test]
    fn classifies_legacy_inline_and_self_and_unknown() {
        let inline = json!({ "mage": { "arete": 3 } });
        assert_eq!(stats_payload(&inline, "mage").shape, "legacy-inline-system");
        assert_eq!(
            stats_payload(&json!({ "vampire": {} }), "self").shape,
            "legacy-self-aggregate"
        );
        assert_eq!(stats_payload(&json!({ "x": 1 }), "mage").shape, "unknown");
    }

    #[test]
    fn summarize_record_derives_rkey_and_fields() {
        let v = json!({ "system": "mage", "data": { "arete": 3, "willpower": 5 } });
        let s = summarize_record("at://did:plc:abc/actor.rpg.stats/mage", Some("cid1"), &v);
        assert_eq!(s.rkey, "mage");
        assert_eq!(s.cid.as_deref(), Some("cid1"));
        assert_eq!(s.shape, "per-system-envelope");
        assert_eq!(s.fields, vec!["arete".to_owned(), "willpower".to_owned()]);
    }

    #[test]
    fn summarize_systems_skips_meta_and_sorts() {
        let v = json!({
            "$type": "actor.rpg.stats",
            "updatedAt": "2026-01-01",
            "mage": { "arete": 3 },
            "core": { "hp": 10 },
        });
        let systems = summarize_systems(&v);
        let keys: Vec<_> = systems.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec!["core", "mage"]);
    }
}
