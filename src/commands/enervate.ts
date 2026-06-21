import type { CommandRunner } from "gunshi";
import { requireAgent } from "../auth/agent.ts";
import { putTassRecord } from "../atproto/pds.ts";
import { TASS_COLLECTIONS, makeEnervate } from "../atproto/tass.ts";

export const run: CommandRunner = async (ctx) => {
	const { agent, did } = await requireAgent();
	const tass = ctx.values.tass as string;
	const amount = ctx.values.amount as number;

	const record = makeEnervate(tass, amount);
	const result = await putTassRecord(
		agent,
		did,
		TASS_COLLECTIONS.enervate,
		record as unknown as Record<string, unknown>,
	);
	if (ctx.values.json) {
		console.log(JSON.stringify({ uri: result.uri, cid: result.cid, record }));
	} else {
		console.log(`✓ enervated ${amount}q from ${tass}`);
		console.log(`  ${result.uri}`);
	}
};

export default {
	name: "enervate",
	description: "Drain/expend tass — register a withdrawal of current",
	args: {
		tass: {
			type: "positional",
			required: true,
			description: "AT-URI of the tassilize record to drain",
		},
		amount: {
			type: "positional",
			required: true,
			description: "Quintessence withdrawn",
		},
		json: {
			type: "boolean",
			short: "j",
			default: false,
		},
	},
	run,
};
