import type { CommandRunner } from "gunshi";
import { login } from "../auth/agent.ts";
import { loadAuthInfo } from "../auth/stores/file-store.ts";

export const run: CommandRunner = async (ctx) => {
	const handle = ctx.values.handle as string | undefined;
	if (!handle) {
		console.error("handle is required. usage: tassle login <handle>");
		process.exit(1);
	}
	const previous = loadAuthInfo();
	console.log(`logging in as ${handle}...`);
	try {
		const { did, handle: resolved } = await login(handle);
		if (previous && previous.did !== did) {
			console.log(`(switched from ${previous.handle})`);
		}
		console.log(`✓ logged in as ${resolved} (${did})`);
	} catch (err) {
		console.error(
			`login failed: ${err instanceof Error ? err.message : String(err)}`,
		);
		process.exit(1);
	}
};

export default {
	name: "login",
	description: "Log in via AT Protocol OAuth (opens browser)",
	args: {
		handle: {
			type: "positional",
			required: true,
			description: "Your AT Protocol handle (e.g. jauntywk.bsky.social)",
		},
	},
	run,
};
