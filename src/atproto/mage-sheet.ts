import type { Agent } from "@atproto/api";
import { DEV_REFERENCE } from "../config.ts";

/**
 * The nine Spheres of Magick in Mage: The Ascension.
 * `prime` is the sphere of working quintessence — central to tassle.
 */
export const SPHERES = [
	"correspondence",
	"entropy",
	"forces",
	"life",
	"matter",
	"mind",
	"prime",
	"spirit",
	"time",
] as const;
export type Sphere = (typeof SPHERES)[number];

/**
 * Subset of actor.rpg.stats#mageStats that tassle cares about.
 * Full sheet has many more fields (attributes, abilities, backgrounds);
 * we type only what we touch and pass the rest through as `unknown`.
 */
export interface MageSheet {
	arete?: number;
	willpower?: number | { current?: number; max?: number };
	quintessence?: number;
	paradox?: number;
	spheres: Partial<Record<Sphere, number>>;
	raw: Record<string, unknown>;
	/** rkey of the record this came from (default: "self"). */
	rkey: string;
	/** Full at-uri of the source record. */
	uri: string;
	updatedAt?: string;
}

const STATS_COLLECTION = DEV_REFERENCE.statsCollection;

type RecordResponse = {
	uri: string;
	value: {
		$type?: string;
		mage?: Record<string, unknown>;
		createdAt?: string;
		updatedAt?: string;
	};
};

function extractSpheres(
	mage: Record<string, unknown>,
): Partial<Record<Sphere, number>> {
	const out: Partial<Record<Sphere, number>> = {};
	for (const s of SPHERES) {
		const v = mage[s];
		if (typeof v === "number") out[s] = v;
	}
	return out;
}

/**
 * Read the mage block from an actor.rpg.stats record.
 *
 * Defaults to jauntywk.bsky.social's "self" rkey for development, but accepts
 * any DID/rkey. The mage block is one of several game systems stored under
 * the same record (mage, cyberpunk2020, dcc, ...).
 */
export async function fetchMageSheet(
	agent: Agent,
	opts: { did: string; rkey?: string } = { did: DEV_REFERENCE.did },
): Promise<MageSheet | null> {
	const rkey = opts.rkey ?? DEV_REFERENCE.statsRkey;
	const res = await agent.com.atproto.repo.getRecord({
		repo: opts.did,
		collection: STATS_COLLECTION,
		rkey,
	});
	const data = res.data as RecordResponse;
	const mage = data.value.mage;
	if (!mage) return null;

	return {
		rkey,
		uri: data.uri,
		spheres: extractSpheres(mage),
		arete: typeof mage.arete === "number" ? mage.arete : undefined,
		willpower:
			typeof mage.willpower === "number"
				? mage.willpower
				: (mage.willpower as { current?: number; max?: number } | undefined),
		quintessence:
			typeof mage.quintessence === "number" ? mage.quintessence : undefined,
		paradox: typeof mage.paradox === "number" ? mage.paradox : undefined,
		updatedAt: data.value.updatedAt,
		raw: mage,
	};
}

/**
 * Patch the mage block of an actor.rpg.stats record.
 *
 * Does a read-modify-write: fetches current record, deep-merges the patch
 * into the mage block, writes the full record back. Caller is responsible
 * for not stomping concurrent edits — for high-frequency writes prefer an
 * append-only action record (see tass.ts).
 */
export async function patchMageSheet(
	agent: Agent,
	did: string,
	patch: Record<string, unknown>,
	opts: { rkey?: string } = {},
): Promise<void> {
	const rkey = opts.rkey ?? DEV_REFERENCE.statsRkey;
	const existing = await agent.com.atproto.repo.getRecord({
		repo: did,
		collection: STATS_COLLECTION,
		rkey,
	});
	const value = existing.data.value as Record<string, unknown> & {
		updatedAt?: string;
	};
	const mage = (value.mage ?? {}) as Record<string, unknown>;
	value.mage = { ...mage, ...patch };
	value.updatedAt = new Date().toISOString();
	await agent.com.atproto.repo.putRecord({
		repo: did,
		collection: STATS_COLLECTION,
		rkey,
		record: value,
	});
}
