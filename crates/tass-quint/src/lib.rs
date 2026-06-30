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

    /// Construct from whole points given as a float, rounded to the nearest
    /// milli. The cast saturates: NaN maps to 0, ±inf clamp to the `i64` bounds.
    /// Callers surfacing user input may want to reject non-finite values first.
    pub fn from_points_f64(points: f64) -> Self {
        Self((points * PER_POINT as f64).round() as i64)
    }

    /// Return a new `Quint` with `delta_millis` added, saturating at the `i64`
    /// bounds. Useful for increment flows:
    /// `current.add_millis(delta.millis())`.
    pub const fn add_millis(self, delta_millis: i64) -> Self {
        Self(self.0.saturating_add(delta_millis))
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

// ─────────────────────────────────────────────────────────────────────────────
// Coherence — inline sync seam for `milliQuintessence` ↔ `quintessence`.
//
// This module is pure logic with no IO. [`QuintClient`] in `tass-quint-jac`
// consults it inline inside its existing `read` / `write_with` / `adjust_with`
// paths so callers get a coherent value back without any separate sync pass.
// See `doc/microquint.md` and the `tass-quint-stale-sync` ticket.
// ─────────────────────────────────────────────────────────────────────────────

/// A drift classification produced by [`Coherence::classify`]. Exhaustive
/// on purpose: it is the *complete* list of repair actions the inline
/// read/write path can take. Adding a new case is a breaking change to the
/// seam, by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    /// The two fields agree and the timestamps are consistent. No refresh.
    InSync,
    /// The legacy `quintessence` field disagrees with
    /// `floor(milliQuintessence / 1000)`, OR the record-level `updatedAt` is
    /// newer than `milliQuintessenceUpdatedAt` (something else mutated the
    /// mage block outside a tass-quint write). Repair by re-replicating the
    /// floor into `quintessence` — the default milli-is-truth action.
    RefreshFloor,
    /// The milli field is present but is no longer the system of record (the
    /// `tass-quint-sync-direction` sibling's `QuintessenceIsTruth` policy).
    /// Repair by hydrating `milliQuintessence = quintessence * 1000` from the
    /// legacy integer. The default coherence impl here never returns this —
    /// it exists so the seam does not need to be reshaped when the sibling
    /// ticket lands.
    HydrateMilli,
}

impl SyncDecision {
    /// True when the sheet is coherent — no repair needed. Only
    /// [`SyncDecision::InSync`] qualifies today.
    pub const fn is_in_sync(self) -> bool {
        matches!(self, SyncDecision::InSync)
    }

    /// True when drift was detected. Convenience inverse of
    /// [`SyncDecision::is_in_sync`]; surfaces in read reports.
    pub const fn is_drift(self) -> bool {
        !self.is_in_sync()
    }
}

/// The raw fields a [`Coherence`] impl looks at. Pulled out of the mage block
/// by the jac layer and handed to the seam; the seam never touches the wire
/// directly, which keeps it unit-testable without a PDS.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetFields<'a> {
    /// Raw `milliQuintessence` (the Tassle extension field), `None` when the
    /// sheet only carries the legacy integer.
    pub milli_quintessence: Option<i64>,
    /// Raw `quintessence` (legacy whole points), `None` when not present.
    pub quintessence: Option<i64>,
    /// `milliQuintessenceUpdatedAt` — when the milli value last changed (the
    /// narrow stamp tass-quint writes). `None` on legacy sheets and on writes
    /// that didn't set it.
    pub milli_quintessence_updated_at: Option<&'a str>,
    /// Record-level `updatedAt` — broad stamp advanced on any mutation of the
    /// record, including ones outside tass-quint. `None` when absent.
    pub updated_at: Option<&'a str>,
}

/// Pluggable coherence rule. The default impl is [`MilliIsTruthCoherence`],
/// matching today's behavior exactly.
///
/// The only callers of this trait live inside `tass-quint-jac`'s existing
/// `read` / `write_with` / `adjust_with` methods. Nothing outside the quint
/// family calls it directly — it is a seam for varying drift heuristics per
/// chronicle, not a new sync verb.
pub trait Coherence {
    /// Inspect `fields` and return the [`SyncDecision`] the inline path should
    /// act on.
    fn classify(&self, fields: &SheetFields<'_>) -> SyncDecision;
}

/// The default coherence rule. Matches today's behavior with no policy
/// applied: `milliQuintessence` is the source of truth.
///
/// Detection logic:
/// 1. If no milli field is present → [`SyncDecision::InSync`] (the legacy
///    integer is the only signal, resolved via [`resolve`]; no coherence
///    check applies — the sheet is as coherent as it can be).
/// 2. If `floor(milli / 1000) != quintessence` → [`SyncDecision::RefreshFloor`]
///    (floor drifted — covers "quintessence disagrees with the milli floor"
///    and "quintessence absent while milli present", the latter being a legacy
///    sheet that needs a replicated floor).
/// 3. Else if `milli_quintessence_updated_at` is older (lexically) than the
///    record-level `updated_at` → [`SyncDecision::RefreshFloor`] too
///    (timestamp drift: something else touched the mage block without a
///    tass-quint write). The repair action is the same floor re-replication;
///    the broad `updatedAt` advance is reported as drift, not acted on as an
///    authority switch.
/// 4. Else → [`SyncDecision::InSync`].
///
/// Timestamp comparison is lexical ISO-8601 string compare, the only format
/// tass-quint writes. A non-ISO broad stamp will compare wrongly; it surfaces
/// as drift via the same path (false positives are safe — the repair is a
/// no-op floor re-replication).
///
/// This impl never returns [`SyncDecision::HydrateMilli`] — that variant is
/// the sibling `tass-quint-sync-direction` ticket's concern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MilliIsTruthCoherence;

impl Coherence for MilliIsTruthCoherence {
    fn classify(&self, fields: &SheetFields<'_>) -> SyncDecision {
        let Some(millis) = fields.milli_quintessence else {
            return SyncDecision::InSync;
        };
        let floor = Quint::from_millis(millis).points();
        if fields.quintessence == Some(floor) {
            // Floor agrees — check timestamp drift.
            if let (Some(milli_ts), Some(rec_ts))
                = (fields.milli_quintessence_updated_at, fields.updated_at)
                && rec_ts > milli_ts
            {
                return SyncDecision::RefreshFloor;
            }
            SyncDecision::InSync
        } else {
            // Floor drifted (covers both "quintessence != floor(milli)" and
            // "quintessence absent while milli present" — the latter is a
            // legacy sheet that needs a replicated floor).
            SyncDecision::RefreshFloor
        }
    }
}

/// The return shape of `QuintClient::read` once coherence is folded in: the
/// coherent [`Quint`] (already repaired per the active coherence policy) plus
/// the [`SyncDecision`] that was made, so callers can surface drift without
/// the read path issuing a write.
///
/// A read MUST NOT mutate the sheet — the `quint` returned here is the coherent
/// view of what's on the wire; the next `write_with` is what repairs the sheet
/// in storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadReport {
    /// The coherent value per the active coherence policy.
    pub quint: Quint,
    /// What the coherence check found. [`SyncDecision::InSync`] when the sheet
    /// was already coherent; otherwise the repair action that produced `quint`.
    pub decision: SyncDecision,
}

impl ReadReport {
    /// True when the sheet was found coherent at read time.
    pub const fn in_sync(&self) -> bool {
        self.decision.is_in_sync()
    }
    /// True when drift was detected (any non-in-sync decision).
    pub const fn drifted(&self) -> bool {
        self.decision.is_drift()
    }
}

/// Resolve the coherent [`Quint`] for a sheet given the coherence decision.
/// Pure: `read` and `adjust_with`'s internal read step both call this.
///
/// - [`SyncDecision::InSync`] or [`SyncDecision::RefreshFloor`]: resolve via
///   [`resolve`] (milli preferred, hydrate from legacy when milli absent). The
///   `RefreshFloor` decision says the *sheet's floor field* needs repair, not
///   the value we return — the value we return is already the milli one.
/// - [`SyncDecision::HydrateMilli`]: hydrate `Quint::from_points(quintessence)`
///   from the legacy integer. This is the forward-compat branch the
///   `tass-quint-sync-direction` sibling ticket's coherence impl will trigger;
///   the default coherence impl never produces this decision today.
/// - `None` when no field is present.
pub fn coherent_quint(fields: &SheetFields<'_>, decision: SyncDecision) -> Option<Quint> {
    match decision {
        SyncDecision::HydrateMilli => fields.quintessence.map(Quint::from_points),
        _ => resolve(fields.milli_quintessence, fields.quintessence),
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

    #[test]
    fn from_points_f64_rounds_to_millis() {
        assert_eq!(Quint::from_points_f64(3.5).millis(), 3_500);
        assert_eq!(Quint::from_points_f64(2.0).millis(), 2_000);
        assert_eq!(Quint::from_points_f64(-0.25).millis(), -250);
        assert_eq!(Quint::from_points_f64(0.0).millis(), 0);
        // non-finite saturates rather than UB
        assert!(Quint::from_points_f64(f64::INFINITY).is_out_of_range());
        assert_eq!(Quint::from_points_f64(f64::NAN).millis(), 0);
    }

    #[test]
    fn add_millis_saturates_and_handles_negative() {
        assert_eq!(Quint::from_millis(1_000).add_millis(500).millis(), 1_500);
        assert_eq!(Quint::from_millis(1_000).add_millis(-250).millis(), 750);
        assert_eq!(Quint::from_millis(i64::MAX).add_millis(1).millis(), i64::MAX);
        assert_eq!(Quint::from_millis(0).add_millis(-1).millis(), -1);
    }

    // — coherence seam — milli-is-truth default ———————————————

    fn fields<'a>(
        milli: Option<i64>,
        quint: Option<i64>,
        milli_ts: Option<&'a str>,
        rec_ts: Option<&'a str>,
    ) -> SheetFields<'a> {
        SheetFields {
            milli_quintessence: milli,
            quintessence: quint,
            milli_quintessence_updated_at: milli_ts,
            updated_at: rec_ts,
        }
    }

    #[test]
    fn coherence_in_sync_when_floor_agrees_and_timestamps_consistent() {
        let f = fields(Some(1_500), Some(1), Some("2026-06-29T10:00:00Z"), Some("2026-06-29T10:00:00Z"));
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
        // broad stamp older than the milli stamp is fine (we wrote after)
        let f = fields(Some(1_500), Some(1), Some("2026-06-29T11:00:00Z"), Some("2026-06-29T10:00:00Z"));
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
    }

    #[test]
    fn coherence_refresh_floor_when_floor_drifted() {
        // quintessence says 9 but floor(1500/1000) = 1
        let f = fields(Some(1_500), Some(9), None, None);
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::RefreshFloor);
    }

    #[test]
    fn coherence_refresh_floor_when_legacy_field_absent_but_milli_present() {
        // legacy field missing — needs a replicated floor
        let f = fields(Some(1_500), None, None, None);
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::RefreshFloor);
    }

    #[test]
    fn coherence_refresh_floor_on_timestamp_drift_with_agreeing_floor() {
        // floor agrees (milli=1500, quint=1) but broad updatedAt is newer
        let f = fields(Some(1_500), Some(1), Some("2026-06-29T10:00:00Z"), Some("2026-06-30T10:00:00Z"));
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::RefreshFloor);
    }

    #[test]
    fn coherence_in_sync_when_no_milli_field() {
        // legacy-only sheet: resolve() handles it, coherence is a no-op
        let f = fields(None, Some(7), None, Some("2026-06-29T10:00:00Z"));
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
    }

    #[test]
    fn coherence_in_sync_when_both_absent() {
        let f = fields(None, None, None, None);
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
    }

    #[test]
    fn coherent_quint_returns_milli_value_on_refresh_floor() {
        let f = fields(Some(1_500), Some(9), None, None);
        let d = MilliIsTruthCoherence.classify(&f);
        assert_eq!(coherent_quint(&f, d), Some(Quint::from_millis(1_500)));
    }

    #[test]
    fn coherent_quint_hydrates_from_legacy_on_hydrate_milli() {
        // forward-compat: a future QuintessenceIsTruth coherence impl would
        // return HydrateMilli; coherent_quint honors it by hydrating from the
        // legacy integer. The default MilliIsTruthCoherence never produces this
        // decision today; this test pins the forward-compat behavior.
        let f = fields(Some(1_500), Some(9), None, None);
        assert_eq!(coherent_quint(&f, SyncDecision::HydrateMilli), Some(Quint::from_points(9)));
    }

    #[test]
    fn coherent_quint_resolves_legacy_only_sheet() {
        let f = fields(None, Some(7), None, None);
        assert_eq!(coherent_quint(&f, SyncDecision::InSync), Some(Quint::from_points(7)));
    }

    #[test]
    fn coherent_quint_none_when_no_fields() {
        let f = fields(None, None, None, None);
        assert_eq!(coherent_quint(&f, SyncDecision::InSync), None);
    }

    #[test]
    fn read_report_helpers_classify_drift() {
        let r = ReadReport { quint: Quint::from_millis(1_500), decision: SyncDecision::RefreshFloor };
        assert!(r.drifted());
        assert!(!r.in_sync());
        let r = ReadReport { quint: Quint::from_millis(1_500), decision: SyncDecision::InSync };
        assert!(!r.drifted());
        assert!(r.in_sync());
    }
}
