import type { CommandRunner } from "gunshi";
import { requireAgent } from "../auth/agent.ts";
import { putTassRecord } from "../atproto/pds.ts";
import { TASS_COLLECTIONS, node } from "../atproto/tass.ts";

export const run: CommandRunner = async (ctx) => {
	const { agent, did } = await requireAgent();

	const record = node()
		.name(ctx.values.name as string)
		.rating(Number(ctx.values.rating))
		.description(ctx.values.description as string | undefined)
		.location(ctx.values.location as string | undefined)
		.resonance(ctx.values.resonance as string | undefined)
		.tassForm(ctx.values.tassForm as string | undefined)
		.ambientQuintessence(
			ctx.values.ambientQuintessence === undefined
				? undefined
				: Number(ctx.values.ambientQuintessence),
		)
		.build();

	const result = await putTassRecord(
		agent,
		did,
		TASS_COLLECTIONS.node,
		record as unknown as Record<string, unknown>,
	);
	if (ctx.values.json) {
		console.log(JSON.stringify({ uri: result.uri, cid: result.cid, record }));
	} else {
		console.log(
			`✓ minted Node "${record.name}" (rating ${record.rating}, ${record.ambientQuintessence}q ambient)`,
		);
		console.log(`  ${result.uri}`);
	}
};

export default {
	name: "mint",
	description: "Mint a new Node — a place where quintessence gathers",
	args: {
		name: {
			type: "positional",
			required: true,
			description: "Display name for the Node",
		},
		rating: {
			type: "string",
			short: "r",
			required: true,
			description: "Node rating 1-5 (determines max ambient quintessence)",
		},
		description: {
			type: "string",
			short: "d",
			description: "Freeform description — appearance, feel, history",
		},
		location: {
			type: "string",
			short: "l",
			description: "Where in the world this Node sits",
		},
		resonance: {
			type: "string",
			short: "R",
			description:
				"Resonance type (dynamic, static, primordial, pattern, questing)",
		},
		tassForm: {
			type: "string",
			short: "f",
			description: "Coincidental form tass takes at this Node",
		},
		ambientQuintessence: {
			type: "string",
			short: "q",
			description:
				"Override the default ambient quintessence (rating * 5)",
		},
		json: {
			type: "boolean",
			short: "j",
			default: false,
		},
	},
	run,
};
