//! Quint — thousandths-of-a-point mage pattern-quintessence.
//!
//! `actor.rpg.stats#mageStats.quintessence` (the rpg.actor host record) is an
//! integer 0-20. Tassle extends the mage block with an optional
//! `milliQuintessence` integer that holds the real, sub-point-resolution
//! balance at milli (1/1000th) granularity; the legacy `quintessence` field
//! becomes a derived display value that always shows the [`Quint::points`] floor.
//!
//! Three properties every sheet read/write should satisfy, encoded here so
//! callers get them "peacefully, without thinking about it":
//!
//! 1. **`milliQuintessence` is the source of truth.** [`resolve`] prefers the
//!    explicit `milliQuintessence` field and only hydrates from
//!    `quintessence * 1000` when it is absent (legacy sheets written before
//!    this extension).
//! 2. **quintessence always shows the rounded-down value.** [`Quint::points`]
//!    is integer division by [`PER_POINT`].
//! 3. **writes replicate.** [`sheet_patch`] emits both fields so clients that
//!    only know the legacy integer stay consistent with the milli balance.
//!
//! This crate is pure logic + [`serde`] adapters — no jacquard, no IO. The
//! companion `tass-quint-jac` crate wraps a jacquard `XrpcClient` to read and
//! write these fields against a PDS. See `doc/ledger.md` for why the ledger
//! must not *silently* mutate `actor.rpg.stats` — sheet writes through
//! [`sheet_patch`] are an explicit command, not a ledger side-effect.

use serde::{Deserialize, Serialize};

/// Milli-quintessence per whole point. `1 point == 1000 milli`.
pub const PER_POINT: i64 = 1_000;

/// Canonical mage pattern-quintessence cap, in whole points.
/// Mirrors `actor.rpg.stats#mageStats.quintessence.maximum` (= 20).
pub const MAX_POINTS: i64 = 20;

/// Same cap expressed in milli-quintessence.
pub const MAX_MILLIS: i64 = MAX_POINTS * PER_POINT;

/// A quantity of mage pattern-quintessence stored at thousandth-of-a-point
/// resolution.
///
/// The wrapped integer is the source of truth; the whole-point view is always
/// the floor of `millis / 1000`. Construction is infallible so anomalous sheet
/// values (negative, over cap) surface as-is rather than being silently
/// clamped — see the "Negative balances are anomalies, not hidden failures"
/// rule in `doc/ledger.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quint(i64);

impl Quint {
    /// Construct from a raw milli-quintessence value.
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Construct from whole points. `from_points(q) == from_millis(q * 1000)`.
    pub const fn from_points(points: i64) -> Self {
        Self(points * PER_POINT)
    }

    /// Raw milli-quintessence — the source-of-truth value to persist in the
    /// `milliQuintessence` sheet field.
    pub const fn millis(self) -> i64 {
        self.0
    }

    /// Whole points, rounded down. This is what the legacy `quintessence`
    /// field and any player-facing display must show.
    pub const fn points(self) -> i64 {
        self.0 / PER_POINT
    }

    /// The sub-point remainder, in `[0, 1000)`. Useful for displays that want
    /// to show the fractional part.
    pub const fn remainder(self) -> i64 {
        self.0 % PER_POINT
    }

    /// True when the wrapped value is outside `[0, MAX_MILLIS]`.
    pub fn is_out_of_range(self) -> bool {
        self.0 < 0 || self.0 > MAX_MILLIS
    }
}

/// Resolve the effective quint from raw mage-block fields.
///
/// Prefer an explicit `milliQuintessence` (the Tassle extension field); hydrate
/// from `quintessence * 1000` when only the legacy integer is present; return
/// `None` when neither field is set.
pub fn resolve(milli_quintessence: Option<i64>, quintessence: Option<i64>) -> Option<Quint> {
    if let Some(millis) = milli_quintessence {
        return Some(Quint::from_millis(millis));
    }
    quintessence.map(Quint::from_points)
}

/// The mage-block patch produced by [`sheet_patch`] — the exact fields to
/// merge into an `actor.rpg.stats` mage record.
///
/// Serialized `camelCase` to match the sheet's wire shape: `milliQuintessence`
/// carries the source-of-truth millis, `quintessence` carries the replicated
/// floor, and `milliQuintessenceUpdatedAt` (when stamped by the write layer
/// via [`SheetPatch::with_updated_at`]) records when the milli value changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetPatch {
    /// Source-of-truth milli-quintessence.
    pub milli_quintessence: i64,
    /// Floored whole points, replicated so legacy clients stay consistent.
    pub quintessence: i64,
    /// When the `milliQuintessence` value was last changed (ISO-8601). `None`
    /// on the pure [`sheet_patch`] builder (which has no clock); the write
    /// layer stamps it with "now". Skipped from the wire when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milli_quintessence_updated_at: Option<String>,
}

impl SheetPatch {
    /// Stamp the patch with the time the milli value was changed. The write
    /// layer calls this with "now" so `milliQuintessenceUpdatedAt` tracks the
    /// last milli-value write on the sheet.
    pub fn with_updated_at(mut self, ts: impl Into<String>) -> Self {
        self.milli_quintessence_updated_at = Some(ts.into());
        self
    }
}

/// Build the mage-block patch for a desired balance: writes `q` as the source
/// of truth and replicates the floored value into the legacy `quintessence`
/// field. Leaves `milliQuintessenceUpdatedAt` unset; the write layer stamps it.
pub fn sheet_patch(q: Quint) -> SheetPatch {
    SheetPatch {
        milli_quintessence: q.millis(),
        quintessence: q.points(),
        milli_quintessence_updated_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_always_floors() {
        assert_eq!(Quint::from_millis(0).points(), 0);
        assert_eq!(Quint::from_millis(999).points(), 0);
        assert_eq!(Quint::from_millis(1_000).points(), 1);
        assert_eq!(Quint::from_millis(1_999).points(), 1);
        assert_eq!(Quint::from_millis(20_000).points(), 20);
    }

    #[test]
    fn from_points_round_trip() {
        let q = Quint::from_points(7);
        assert_eq!(q.millis(), 7_000);
        assert_eq!(q.points(), 7);
        assert_eq!(q.remainder(), 0);
    }

    #[test]
    fn remainder_stays_in_unit_range() {
        for millis in [0i64, 1, 999, 1_000, 1_001, 19_999, 20_000] {
            let r = Quint::from_millis(millis).remainder();
            assert!(
                (0..PER_POINT).contains(&r),
                "remainder {r} out of range for millis {millis}"
            );
        }
    }

    #[test]
    fn resolve_prefers_explicit_quint() {
        let resolved = resolve(Some(1_500), Some(9));
        assert_eq!(resolved, Some(Quint::from_millis(1_500)));
        assert_eq!(resolved.unwrap().points(), 1);
    }

    #[test]
    fn resolve_hydrates_from_legacy_quintessence() {
        let resolved = resolve(None, Some(9));
        assert_eq!(resolved, Some(Quint::from_points(9)));
        assert_eq!(resolved.unwrap().millis(), 9_000);
        assert_eq!(resolved.unwrap().points(), 9);
    }

    #[test]
    fn resolve_none_when_both_absent() {
        assert_eq!(resolve(None, None), None);
    }

    #[test]
    fn sheet_patch_replicates_floored_points() {
        let patch = sheet_patch(Quint::from_millis(1_500));
        assert_eq!(patch.milli_quintessence, 1_500);
        assert_eq!(patch.quintessence, 1);
    }

    #[test]
    fn sheet_patch_serializes_camel_case() {
        let patch = sheet_patch(Quint::from_points(3));
        let json = serde_json::to_string(&patch).unwrap();
        // updatedAt stays absent when the write layer hasn't stamped it.
        assert_eq!(json, r#"{"milliQuintessence":3000,"quintessence":3}"#);
    }

    #[test]
    fn sheet_patch_with_updated_at_serializes() {
        let patch = sheet_patch(Quint::from_points(3)).with_updated_at("2026-06-29T21:00:00Z");
        let json = serde_json::to_string(&patch).unwrap();
        assert_eq!(
            json,
            r#"{"milliQuintessence":3000,"quintessence":3,"milliQuintessenceUpdatedAt":"2026-06-29T21:00:00Z"}"#
        );
    }

    #[test]
    fn serde_transparent_integer() {
        let q = Quint::from_millis(12_345);
        let json = serde_json::to_string(&q).unwrap();
        assert_eq!(json, "12345");
        let back: Quint = serde_json::from_str("12345").unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn anomalies_surface_not_clamped() {
        let neg = Quint::from_millis(-1);
        let over = Quint::from_millis(MAX_MILLIS + 1);
        assert!(neg.is_out_of_range());
        assert!(over.is_out_of_range());
        // not clamped: values pass through verbatim
        assert_eq!(neg.millis(), -1);
        assert_eq!(over.millis(), MAX_MILLIS + 1);
    }
}
