import type { Agent } from "@atproto/api";
import { DEV_REFERENCE } from "../config.ts";

/**
 * The nine Spheres of Magick in Mage: The Ascension.
 * `prime` is the sphere of working quintessence — central to tassle.
 *
 * NB: the actor.rpg.stats lexicon declares these in lowercase, but legacy
 * `self` rkey records (including the dev-reference sheet) use Capitalized
 * keys, and `Forces` appears as singular `Force`. We accept either form on
 * read; writes use the lexicon-canonical lowercase.
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

/** Aliases seen in real records (legacy `self` rkey uses Capitalized + `Force`). */
const SPHERE_ALIASES: Record<string, Sphere> = {
	correspondence: "correspondence",
	Correspondence: "correspondence",
	entropy: "entropy",
	Entropy: "entropy",
	forces: "forces",
	Forces: "forces",
	force: "forces",
	Force: "forces",
	life: "life",
	Life: "life",
	matter: "matter",
	Matter: "matter",
	mind: "mind",
	Mind: "mind",
	prime: "prime",
	Prime: "prime",
	spirit: "spirit",
	Spirit: "spirit",
	time: "time",
	Time: "time",
};

/** Same case-tolerance for advantage keys. */
function pick(
	mage: Record<string, unknown>,
	...candidates: string[]
): number | undefined {
	for (const k of candidates) {
		const v = mage[k];
		if (typeof v === "number") return v;
	}
	return undefined;
}

/**
 * Subset of actor.rpg.stats#mageStats that tassle cares about.
 * Full sheet has many more fields (attributes, abilities, backgrounds);
 * we type only what we touch and pass the rest through as `raw`.
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
	for (const [k, v] of Object.entries(mage)) {
		const canonical = SPHERE_ALIASES[k];
		if (canonical && typeof v === "number") out[canonical] = v;
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
		arete: pick(mage, "arete", "Arete"),
		willpower: pick(mage, "willpower", "Willpower"),
		quintessence: pick(mage, "quintessence", "Quintessence"),
		paradox: pick(mage, "paradox", "Paradox"),
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
 *
 * Patches should use lexicon-canonical lowercase keys.
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
