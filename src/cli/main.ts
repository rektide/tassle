import { cli } from "gunshi";
import { renderValidationErrors } from "gunshi/renderer";
import completion from "@gunshi/plugin-completion";

import login from "../commands/login.ts";
import logout from "../commands/logout.ts";
import whoami from "../commands/whoami.ts";
import sheet from "../commands/sheet.ts";
import mint from "../commands/mint.ts";
import tassilize from "../commands/tassilize.ts";
import meditate from "../commands/meditate.ts";
import enervate from "../commands/enervate.ts";
import samples from "../commands/samples.ts";

const entry = {
	name: "tassle",
	description:
		"Tassle — Mage: The Ascension quintessence/tass energy ledger on atproto (rpg.actor)",
		run: () => {
		console.log("tassle: an energy ledger for rpg.actor mages");
		console.log("\nCommands:");
		console.log("  login <handle>   authenticate via OAuth");
		console.log("  logout           clear session");
		console.log("  whoami           show current user");
		console.log("  sheet            read your mage character sheet");
		console.log("  mint             mint a Node");
		console.log("  tassilize        crystallize quintessence into tass");
		console.log("  meditate         draw quintessence from a node");
		console.log("  enervate         drain tass");
		console.log("  samples          generate example records into samples/");
	},
};

export async function runCli(argv: string[]): Promise<void> {
	try {
		await cli(argv, entry, {
			name: "tassle",
			version: "1.0.0",
			plugins: [completion()],
			// Suppress the per-command banner; we only want output from `run`.
			// Header still appears on `--help` via the usage renderer.
			renderHeader: null,
			// Wire the validation-error renderer so missing required args print
			// a clean usage message instead of throwing an AggregateError stack.
			renderValidationErrors,
		subCommands: {
			login,
			logout,
			whoami,
			sheet,
			mint,
			tassilize,
			meditate,
			enervate,
			samples,
		},
		});
	} catch (err) {
		// gunshi renders validation errors via renderValidationErrors above,
		// then re-throws the AggregateError. Swallow cleanly.
		if (err instanceof AggregateError) process.exit(1);
		throw err;
	}
}
