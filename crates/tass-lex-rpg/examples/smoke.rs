// Smoke test for generated rpg types — verifies the bindings compile and
// the lexicon_doc round-trips.

use jacquard_common::DefaultStr;
use jacquard_lexicon::schema::LexiconSchema;
use tass_lex_rpg::actor_rpg::stats::Stats;
use tass_lex_rpg::equipment_rpg::item::Item;

fn main() {
	// Verify each main record type exposes its lexicon doc.
	let stats_doc = Stats::<DefaultStr>::lexicon_doc();
	println!("{}: {} defs", stats_doc.id, stats_doc.defs.len());

	let item_doc = Item::<DefaultStr>::lexicon_doc();
	println!("{}: {} defs", item_doc.id, item_doc.defs.len());

	assert_eq!(stats_doc.id, "actor.rpg.stats");
	assert_eq!(item_doc.id, "equipment.rpg.item");
	println!("✓ rpg bindings OK");
}
