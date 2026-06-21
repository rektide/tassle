/**
 * Tass collections and their record builders.
 *
 * Each collection is its own NSID under com.superbfowle.tass.* — see
 * lexicons/ for the JSON schemas. Three actions target a Node (an at-uri
 * to a com.superbfowle.tass.node record, or eventually any "place where
 * quintessence gathers") and carry an ISO timestamp for per-action LWW.
 *
 * Builders are fluent (bon-style): required fields are fluent setters that
 * validate; `build()` returns the typed record with createdAt filled in.
 *
 *   const n = node().name("Crystal Spring").rating(3).resonance("dynamic").build()
 *   const t = tassilize().node(n.uri).quintessence(5).form("a silver coin").build()
 */

export const TASS_COLLECTIONS = {
	node: "com.superbfowle.tass.node",
	tassilize: "com.superbfowle.tass.tassilize",
	meditate: "com.superbfowle.tass.meditate",
	enervate: "com.superbfowle.tass.enervate",
} as const;

export type TassCollection =
	(typeof TASS_COLLECTIONS)[keyof typeof TASS_COLLECTIONS];

function isoNow(): string {
	return new Date().toISOString();
}

function requireInt(
	value: number | undefined,
	name: string,
	opts: { min?: number; max?: number } = {},
): void {
	if (value === undefined) throw new Error(`${name} is required`);
	if (!Number.isInteger(value)) throw new Error(`${name} must be an integer`);
	if (opts.min !== undefined && value < opts.min) {
		throw new Error(`${name} must be >= ${opts.min}, got ${value}`);
	}
	if (opts.max !== undefined && value > opts.max) {
		throw new Error(`${name} must be <= ${opts.max}, got ${value}`);
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// Node
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Node: a place where quintessence naturally gathers.
 * The target of meditate and the anchor for tassilize.
 */
export interface NodeRecord {
	name: string;
	description?: string;
	location?: string;
	rating: number; // 1-5
	ambientQuintessence?: number; // defaults to rating * 5
	resonance?: string;
	tassForm?: string;
	createdAt: string;
}

export class NodeBuilder {
	private _name?: string;
	private _rating?: number;
	private _description?: string;
	private _location?: string;
	private _ambientQuintessence?: number;
	private _resonance?: string;
	private _tassForm?: string;

	/** Required. Display name for the Node. */
	name(name: string): this {
		this._name = name;
		return this;
	}

	/** Required. Node background rating, integer 1-5. */
	rating(rating: number): this {
		requireInt(rating, "rating", { min: 1, max: 5 });
		this._rating = rating;
		return this;
	}

	description(description: string | undefined): this {
		if (description !== undefined) this._description = description;
		return this;
	}

	location(location: string | undefined): this {
		if (location !== undefined) this._location = location;
		return this;
	}

	/** Override the default ambient quintessence (rating * 5). */
	ambientQuintessence(q: number | undefined): this {
		if (q !== undefined) requireInt(q, "ambientQuintessence", { min: 0 });
		this._ambientQuintessence = q;
		return this;
	}

	/** Resonance type (e.g. dynamic, static, primordial, pattern, questing). */
	resonance(resonance: string | undefined): this {
		if (resonance !== undefined) this._resonance = resonance;
		return this;
	}

	/** Coincidental form tass naturally takes at this Node. */
	tassForm(form: string | undefined): this {
		if (form !== undefined) this._tassForm = form;
		return this;
	}

	build(): NodeRecord {
		if (this._name === undefined) throw new Error("name is required");
		if (this._rating === undefined) throw new Error("rating is required");
		return {
			name: this._name,
			rating: this._rating,
			description: this._description,
			location: this._location,
			ambientQuintessence:
				this._ambientQuintessence ?? this._rating * 5,
			resonance: this._resonance,
			tassForm: this._tassForm,
			createdAt: isoNow(),
		};
	}
}

/** Start a Node builder. */
export function node(): NodeBuilder {
	return new NodeBuilder();
}

// ─────────────────────────────────────────────────────────────────────────────
// Tassilize
// ─────────────────────────────────────────────────────────────────────────────

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

export class TassilizeBuilder {
	private _node?: string;
	private _quintessence?: number;
	private _form?: string;
	private _note?: string;

	/** Required. AT-URI of the Node where the tass crystallizes. */
	node(node: string): this {
		this._node = node;
		return this;
	}

	/** Required. Initial quintessence value, integer 0-100. */
	quintessence(q: number): this {
		requireInt(q, "quintessence", { min: 0, max: 100 });
		this._quintessence = q;
		return this;
	}

	/** Coincidental form the tass takes in reality. */
	form(form: string | undefined): this {
		if (form !== undefined) this._form = form;
		return this;
	}

	note(note: string | undefined): this {
		if (note !== undefined) this._note = note;
		return this;
	}

	build(): TassilizeRecord {
		if (this._node === undefined) throw new Error("node is required");
		if (this._quintessence === undefined) {
			throw new Error("quintessence is required");
		}
		return {
			node: this._node,
			quintessence: this._quintessence,
			form: this._form,
			note: this._note,
			createdAt: isoNow(),
		};
	}
}

/** Start a Tassilize builder. */
export function tassilize(): TassilizeBuilder {
	return new TassilizeBuilder();
}

// ─────────────────────────────────────────────────────────────────────────────
// Meditate
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Meditate: pull quintessence from a Node's ambiance into the mage's pattern.
 * Reduces the Node's ambient quintessence — recorded, not enforced here.
 */
export interface MeditateRecord {
	node: string;
	amount: number; // quintessence drawn
	createdAt: string;
}

export class MeditateBuilder {
	private _node?: string;
	private _amount?: number;

	/** Required. AT-URI of the Node to meditate at. */
	node(node: string): this {
		this._node = node;
		return this;
	}

	/** Required. Quintessence drawn, integer 0-20. */
	amount(amount: number): this {
		requireInt(amount, "amount", { min: 0, max: 20 });
		this._amount = amount;
		return this;
	}

	build(): MeditateRecord {
		if (this._node === undefined) throw new Error("node is required");
		if (this._amount === undefined) throw new Error("amount is required");
		return {
			node: this._node,
			amount: this._amount,
			createdAt: isoNow(),
		};
	}
}

/** Start a Meditate builder. */
export function meditate(): MeditateBuilder {
	return new MeditateBuilder();
}

// ─────────────────────────────────────────────────────────────────────────────
// Enervate
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Enervate: a registered drain / expenditure of tass.
 * "To enervate" — to draw the sinew (nerve) out; here, to tap the tass and
 * withdraw its current.
 */
export interface EnervateRecord {
	tass: string; // at-uri of the tassilize record being drained
	amount: number; // quintessence withdrawn
	purpose?: string;
	createdAt: string;
}

export class EnervateBuilder {
	private _tass?: string;
	private _amount?: number;
	private _purpose?: string;

	/** Required. AT-URI of the tassilize record being drained. */
	tass(tass: string): this {
		this._tass = tass;
		return this;
	}

	/** Required. Quintessence withdrawn, integer 0-100. */
	amount(amount: number): this {
		requireInt(amount, "amount", { min: 0, max: 100 });
		this._amount = amount;
		return this;
	}

	/** What the withdrawn quintessence was spent on. */
	purpose(purpose: string | undefined): this {
		if (purpose !== undefined) this._purpose = purpose;
		return this;
	}

	build(): EnervateRecord {
		if (this._tass === undefined) throw new Error("tass is required");
		if (this._amount === undefined) throw new Error("amount is required");
		return {
			tass: this._tass,
			amount: this._amount,
			purpose: this._purpose,
			createdAt: isoNow(),
		};
	}
}

/** Start an Enervate builder. */
export function enervate(): EnervateBuilder {
	return new EnervateBuilder();
}
