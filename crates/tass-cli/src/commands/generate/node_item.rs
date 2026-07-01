// `tassle gen node-item` — mint a Node as a mage's owned equipment.
//
// Unlike `gen node` (which emits a standalone `at.telluri.node`), this models
// the Node as an `equipment.rpg.item` living on a mage's PDS. Because an item
// must reference a provider's `equipment.rpg.give` attestation, this emits a
// self-give pair: the mage provisions their own Node. Node-specific fields —
// rating, current quintessence (capped at the rating, milli-tracked via
// tass-quint), and the tass it makes — live in the item's freeform `stats`
// object, exactly the extension point `equipment.rpg.item.stats` exists for.

use clap::Args;
use jacquard_common::DefaultStr;
use jacquard_common::types::datetime::Datetime;
use jacquard_common::types::string::{AtUri, Did};
use jacquard_common::types::tid::Tid;
use jacquard_common::types::value::Data;
use jacquard_lexicon::schema::LexiconSchema;
use miette::IntoDiagnostic;
use serde::Serialize;
use std::process::ExitCode;
use tass_lex_rpg::equipment_rpg::give::Give;
use tass_lex_rpg::equipment_rpg::item::Item;
use tass_quint::{PER_POINT, Quint};

#[derive(Args, Debug)]
pub struct NodeItemArgs {
    /// Display name for the Node (the item's title)
    pub name: String,

    /// DID of the mage who owns this Node (provider + recipient of the self-give)
    #[arg(short, long)]
    pub mage: String,

    /// Node rating 1-5 (also the current-quintessence cap, in whole points)
    #[arg(short, long)]
    pub rating: i64,

    /// Item identifier matching the give record (default: "node")
    #[arg(short, long, default_value = "node")]
    pub item: String,

    /// Current quintessence in whole points (default: full = rating). Capped to the rating.
    #[arg(short, long)]
    pub quintessence: Option<i64>,

    /// Source-of-truth current quintessence in milli-points (overrides --quintessence). Capped to rating*1000.
    #[arg(long)]
    pub milli_quintessence: Option<i64>,

    /// Coincidental form the tass this Node makes takes (e.g. "a silver coin")
    #[arg(short, long)]
    pub tass_form: Option<String>,

    /// Resonance type (dynamic, static, primordial, pattern, questing)
    #[arg(short = 'R', long)]
    pub resonance: Option<String>,

    /// Freeform description / flavour text
    #[arg(short, long)]
    pub description: Option<String>,

    /// Skip schema validation before output
    #[arg(long)]
    pub no_validate: bool,
}

/// The node-specific properties copied into the item's freeform `stats` object.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeStats {
    /// Node rating 1-5; the current-quintessence cap in whole points.
    rating: i64,
    /// Source-of-truth current quintessence, in milli-points (tass-quint).
    milli_quintessence: i64,
    /// Derived whole-point floor of `milli_quintessence`.
    quintessence: i64,
    /// A Node crystallizes tass; always true for a Node item.
    makes_tass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tass_form: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resonance: Option<String>,
}

/// The self-give pair emitted for a one-off Node definition.
#[derive(Serialize)]
struct NodeItemPair {
    /// The provider's attestation (mage gives the Node to themselves).
    give: Give<DefaultStr>,
    /// The owned item referencing that give.
    item: Item<DefaultStr>,
}

pub fn run(args: NodeItemArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    if !(1..=5).contains(&args.rating) {
        eprintln!("rating must be between 1 and 5 (got {})", args.rating);
        return Ok(ExitCode::FAILURE);
    }

    let mage: Did<DefaultStr> = Did::new_owned(&args.mage).into_diagnostic()?;

    // Current quintessence, capped at the rating (in whole points).
    let cap_millis = args.rating * PER_POINT;
    let requested_millis = match (args.milli_quintessence, args.quintessence) {
        (Some(m), _) => m,
        (None, Some(q)) => q * PER_POINT,
        (None, None) => cap_millis, // a fresh, full node
    };
    let quint = Quint::from_millis(requested_millis.clamp(0, cap_millis));

    let stats = NodeStats {
        rating: args.rating,
        milli_quintessence: quint.millis(),
        quintessence: quint.points(),
        makes_tass: true,
        tass_form: args.tass_form.clone(),
        resonance: args.resonance.clone(),
    };
    // Round-trip the typed stats through serde into the lexicon's freeform Data.
    let stats: Data<DefaultStr> =
        serde::Deserialize::deserialize(serde_json::to_value(&stats).into_diagnostic()?)
            .into_diagnostic()?;

    let now = Datetime::from(chrono::Utc::now().fixed_offset());

    // Synthesize the give's AT-URI so the item can reference it internally.
    let give_rkey = Tid::now_0();
    let give_uri = format!("at://{}/equipment.rpg.give/{}", args.mage, give_rkey.as_str());
    let give_uri: AtUri<DefaultStr> = AtUri::new_owned(&give_uri).into_diagnostic()?;

    let mut give_builder = Give::<DefaultStr>::builder()
        .recipient(mage.clone())
        .item(args.item.clone())
        .maybe_kind(Some("node".into()))
        .title(args.name.clone())
        .given_at(now.clone())
        .stats(stats.clone());
    if let Some(d) = args.description.clone() {
        give_builder = give_builder.maybe_description(Some(d.into()));
    }
    let give = give_builder.build();

    let mut item_builder = Item::<DefaultStr>::builder()
        .item(args.item.clone())
        .maybe_kind(Some("node".into()))
        .title(args.name.clone())
        .give(give_uri)
        .provider(mage)
        .accepted_at(now)
        .stats(stats);
    if let Some(d) = args.description {
        item_builder = item_builder.maybe_description(Some(d.into()));
    }
    let item = item_builder.build();

    if !args.no_validate {
        if let Err(err) = give.validate() {
            eprintln!("give validation failed:");
            eprintln!("  {err}");
            return Ok(ExitCode::FAILURE);
        }
        if let Err(err) = item.validate() {
            eprintln!("item validation failed:");
            eprintln!("  {err}");
            return Ok(ExitCode::FAILURE);
        }
    }

    crate::commands::emit(&NodeItemPair { give, item }, format)
}
