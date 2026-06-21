import type { CommandRunner } from "gunshi";
import { requireAgent } from "../auth/agent.ts";
import { putTassRecord } from "../atproto/pds.ts";
import { TASS_COLLECTIONS, makeMeditate } from "../atproto/tass.ts";

export const run: CommandRunner = async (ctx) => {
	const { agent, did } = await requireAgent();
	const node = ctx.values.node as string;
	const amount = ctx.values.amount as number;

	const record = makeMeditate(node, amount);
	const result = await putTassRecord(
		agent,
		did,
		TASS_COLLECTIONS.meditate,
		record as unknown as Record<string, unknown>,
	);
	if (ctx.values.json) {
		console.log(JSON.stringify({ uri: result.uri, cid: result.cid, record }));
	} else {
		console.log(`✓ meditated ${amount}q from ${node}`);
		console.log(`  ${result.uri}`);
	}
};

export default {
	name: "meditate",
	description: "Pull quintessence from a Node's ambiance",
	args: {
		node: {
			type: "positional",
			required: true,
			description: "AT-URI of the node",
		},
		amount: {
			type: "positional",
			required: true,
			description: "Quintessence drawn",
		},
		json: {
			type: "boolean",
			short: "j",
			default: false,
		},
	},
	run,
};
