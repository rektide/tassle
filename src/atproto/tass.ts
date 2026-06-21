/**
 * The three tass action collections. Each is its own NSID under
 * com.superbfowle.tass.* — see lexicons/ for the JSON schemas.
 *
 * All three target a Node (an at-uri to a com.superbfowle.tass.node record,
 * or eventually any "place where quintessence gathers") and carry an
 * ISO timestamp for per-action LWW.
 */

export const TASS_COLLECTIONS = {
	tassilize: "com.superbfowle.tass.tassilize",
	meditate: "com.superbfowle.tass.meditate",
	enervate: "com.superbfowle.tass.enervate",
} as const;

export type TassCollection =
	(typeof TASS_COLLECTIONS)[keyof typeof TASS_COLLECTIONS];

/**
 * Tassilize: genesis record of tass forming at a Node.
 * Records the initial quintessence value the tass was crystallized with.
 */
export interface TassilizeRecord {
	node: string; // at-uri of the node
	quintessence: number; // initial quintessence crystallized
	form?: string; // coincidental form taken (e.g. "a silver coin")
	note?: string;
	createdAt: string;
}

/**
 * Meditate: pull quintessence from a Node's ambiance into the mage's pattern.
 * Reduces the Node's ambient quintessence — recorded, not enforced here.
 */
export interface MeditateRecord {
	node: string;
	amount: number; // quintessence drawn
	createdAt: string;
}

/**
 * Enervate: a registered drain / expenditure of tass.
 * "To enervate" — to draw the sinew (nerve) out; here, to tap the tass and
 * withdraw its current.
 */
export interface EnervateRecord {
	tass: string; // at-uri of the tassilize record being drained
	amount: number; // quintessence withdrawn
	createdAt: string;
}

function isoNow(): string {
	return new Date().toISOString();
}

export function makeTassilize(
	node: string,
	quintessence: number,
	opts: { form?: string; note?: string } = {},
): TassilizeRecord {
	return {
		node,
		quintessence,
		form: opts.form,
		note: opts.note,
		createdAt: isoNow(),
	};
}

export function makeMeditate(node: string, amount: number): MeditateRecord {
	return { node, amount, createdAt: isoNow() };
}

export function makeEnervate(tass: string, amount: number): EnervateRecord {
	return { tass, amount, createdAt: isoNow() };
}
