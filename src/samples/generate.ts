/**
 * Sample record generator.
 *
 * Reads no lexicon files (yet) — instead, calls the fluent builders with
 * canonical illustrative values to produce example records. The eventual
 * goal is a true lexicon-driven generator: read static lexicon JSON from
 * lexicons/, synthesize records from the schema. For now, this is the
 * builder-on-top-of-static-files pattern.
 *
 * Samples are written to samples/<name>.example.json. Each sample is the
 * raw record value (no envelope), suitable for diffing, committing, and
 * discussion. At-URIs use a placeholder DID since authority is undecided.
 */

import { writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import {
	node,
	tassilize,
	meditate,
	enervate,
} from "../atproto/tass.ts";

const SAMPLES_DIR = join(
	dirname(fileURLToPath(import.meta.url)),
	"../../samples",
);

/** Placeholder DID for sample at-uris (not a real publisher). */
export const SAMPLE_DID = "did:plc:samplesamplesamplesample" as const;

/** Placeholder at-uri builder for samples. */
function sampleAtUri(collection: string, rkey: string): string {
	return `at://${SAMPLE_DID}/${collection}/${rkey}`;
}

/** Sample records use a fixed createdAt so diffs are stable. */
export const SAMPLE_CREATED_AT = "2026-06-21T12:00:00.000Z" as const;

/** Replace the rand-bearing createdAt with a fixed value for stable diffs. */
function withFixedCreatedAt<T extends { createdAt: string }>(record: T): T {
	record.createdAt = SAMPLE_CREATED_AT;
	return record;
}

interface SampleFile {
	filename: string;
	description: string;
	record: unknown;
}

// ─────────────────────────────────────────────────────────────────────────────
// Sample definitions
//
// Each sample is a record value (the wire-format JSON, no envelope).
// Edit these to refine the canonical examples; `tassle samples` regenerates
// the JSON files in samples/.
// ─────────────────────────────────────────────────────────────────────────────

const NODE_URIS = {
	// Reference at-uris used by other samples. The rkey is illustrative.
	crystalSpring: sampleAtUri("at.telluri.node", "3ksamplesample1"),
};

const TASSILIZE_URIS = {
	silverCoin: sampleAtUri(
		"at.telluri.act.tassilize",
		"3ksamplesample2",
	),
};

const samples: SampleFile[] = [
	{
		filename: "node-crystal-spring.example.json",
		description: "A rating-3 Node with dynamic resonance and a default ambient pool (15q).",
		record: withFixedCreatedAt(
			node()
				.name("Crystal Spring")
				.rating(3)
				.description("A clear spring deep in the old forest; the water hums faintly to those who can hear.")
				.location("Old-growth forest, three miles north of the caern")
				.resonance("dynamic")
				.tassForm("a smooth river-stone, warm to the touch")
				.build(),
		),
	},
	{
		filename: "tassilize-silver-coin.example.json",
		description: "Genesis record: 5q crystallized at the Crystal Spring node as a silver coin.",
		record: withFixedCreatedAt(
			tassilize()
				.node(NODE_URIS.crystalSpring)
				.quintessence(5)
				.form("a silver coin, untarnished")
				.note("Pulled from the spring's surface at dawn.")
				.build(),
		),
	},
	{
		filename: "meditate-dawn-pull.example.json",
		description: "Meditating at the Crystal Spring, drawing 3q into the mage's pattern.",
		record: withFixedCreatedAt(
			meditate()
				.node(NODE_URIS.crystalSpring)
				.amount(3)
				.build(),
		),
	},
	{
		filename: "enervate-spend.example.json",
		description: "Spending 2q from the silver-coin tass to fuel a coincidence.",
		record: withFixedCreatedAt(
			enervate()
				.tass(TASSILIZE_URIS.silverCoin)
				.amount(2)
				.purpose("Lock the door behind us — looks like it was just unlocked all along.")
				.build(),
		),
	},
];

/**
 * Generate all sample files into samples/.
 * Returns the list of files written.
 */
export function generateAllSamples(): string[] {
	mkdirSync(SAMPLES_DIR, { recursive: true });
	const written: string[] = [];
	for (const sample of samples) {
		const path = join(SAMPLES_DIR, sample.filename);
		writeFileSync(path, JSON.stringify(sample.record, null, 2) + "\n");
		written.push(sample.filename);
	}
	return written;
}

export const sampleIndex = samples.map((s) => ({
	filename: s.filename,
	description: s.description,
}));
