import type { CommandRunner } from "gunshi";
import { requireAgent } from "../auth/agent.ts";
import { putTassRecord } from "../atproto/pds.ts";
import { TASS_COLLECTIONS, tassilize } from "../atproto/tass.ts";

export const run: CommandRunner = async (ctx) => {
	const { agent, did } = await requireAgent();

	const record = tassilize()
		.node(ctx.values.node as string)
		.quintessence(Number(ctx.values.quintessence))
		.form(ctx.values.form as string | undefined)
		.note(ctx.values.note as string | undefined)
		.build();

	const result = await putTassRecord(
		agent,
		did,
		TASS_COLLECTIONS.tassilize,
		record as unknown as Record<string, unknown>,
	);
	if (ctx.values.json) {
		console.log(JSON.stringify({ uri: result.uri, cid: result.cid, record }));
	} else {
		console.log(`✓ tassilized ${record.quintessence}q at ${record.node}`);
		console.log(`  ${result.uri}`);
	}
};

export default {
	name: "tassilize",
	description: "Crystallize quintessence into Tass at a Node (genesis record)",
	args: {
		node: {
			type: "positional",
			required: true,
			description: "AT-URI of the node",
		},
		quintessence: {
			type: "positional",
			required: true,
			description: "Initial quintessence crystallized",
		},
		form: {
			type: "string",
			short: "f",
			description: "Coincidental form taken (e.g. 'a silver coin')",
		},
		note: {
			type: "string",
			short: "n",
			description: "Freeform note",
		},
		json: {
			type: "boolean",
			short: "j",
			default: false,
		},
	},
	run,
};
