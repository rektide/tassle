import type { CommandRunner } from "gunshi";
import { requireAgent } from "../auth/agent.ts";
import { putTassRecord } from "../atproto/pds.ts";
import { TASS_COLLECTIONS, enervate } from "../atproto/tass.ts";

export const run: CommandRunner = async (ctx) => {
	const { agent, did } = await requireAgent();

	const record = enervate()
		.tass(ctx.values.tass as string)
		.amount(Number(ctx.values.amount))
		.purpose(ctx.values.purpose as string | undefined)
		.build();

	const result = await putTassRecord(
		agent,
		did,
		TASS_COLLECTIONS.enervate,
		record as unknown as Record<string, unknown>,
	);
	if (ctx.values.json) {
		console.log(JSON.stringify({ uri: result.uri, cid: result.cid, record }));
	} else {
		console.log(`✓ enervated ${record.amount}q from ${record.tass}`);
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
		purpose: {
			type: "string",
			short: "p",
			description: "What the withdrawn quintessence was spent on",
		},
		json: {
			type: "boolean",
			short: "j",
			default: false,
		},
	},
	run,
};
