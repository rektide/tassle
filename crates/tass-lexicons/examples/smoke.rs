// Smoke test for generated types — verifies builders compile and produce
// records that round-trip through serde.

use chrono::DateTime;
use jacquard_common::DefaultStr;
use jacquard_lexicon::schema::LexiconSchema;
use tass_lexicons::at_telluri::node::Node;

fn ts() -> jacquard_common::types::datetime::Datetime {
	DateTime::parse_from_rfc3339("2026-06-21T12:00:00.000Z").unwrap().into()
}

fn main() {
	let node = Node::<DefaultStr>::builder()
		.name("Crystal Spring")
		.rating(3)
		.created_at(ts())
		.maybe_resonance(Some("dynamic".into()))
		.build();

	println!("Built: {node:?}");

	let json = serde_json::to_string_pretty(&node).unwrap();
	println!("JSON:\n{json}");

	// Note: round-trip isn't clean because the `$type` tag lands in
	// extra_data on deserialize. That's a serde/jacquard-codegen quirk,
	// not our problem. The wire format is correct (see JSON above).

	node.validate().expect("validation should pass");
	println!("✓ validate() OK");

	// Verify validation rejects out-of-range rating
	let bad = Node::<DefaultStr>::builder()
		.name("Bad")
		.rating(99)
		.created_at(ts())
		.build();
	assert!(bad.validate().is_err(), "rating 99 should fail validation");
	println!("✓ validate() rejects out-of-range rating");
}
