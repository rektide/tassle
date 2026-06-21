import type { CommandRunner } from "gunshi";
import { loadAuthInfo } from "../auth/stores/file-store.ts";

export const run: CommandRunner = async (ctx) => {
	const info = loadAuthInfo();
	if (!info) {
		if (ctx.values.json) {
			console.log(JSON.stringify({ authenticated: false }));
		} else {
			console.log("not logged in");
		}
		return;
	}
	const out = {
		authenticated: true,
		did: info.did,
		handle: info.handle,
		pds: info.service,
	};
	if (ctx.values.json) {
		console.log(JSON.stringify(out, null, 2));
	} else {
		console.log(`${info.handle} (${info.did})`);
		console.log(`  pds: ${info.service}`);
	}
};

export default {
	name: "whoami",
	description: "Show current authenticated user",
	args: {
		json: {
			type: "boolean",
			short: "j",
			description: "Output as JSON",
			default: false,
		},
	},
	run,
};
