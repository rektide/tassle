import { cli, lazy } from "gunshi";
import completion from "@gunshi/plugin-completion";

import login from "../commands/login.ts";
import logout from "../commands/logout.ts";
import whoami from "../commands/whoami.ts";

const sheet = lazy(() => import("../commands/sheet.ts"), {
	name: "sheet",
	description: "Read your Mage: The Ascension character sheet",
});
const tassilize = lazy(() => import("../commands/tassilize.ts"), {
	name: "tassilize",
	description: "Crystallize quintessence into Tass at a Node",
});
const meditate = lazy(() => import("../commands/meditate.ts"), {
	name: "meditate",
	description: "Pull quintessence from a Node's ambiance",
});
const enervate = lazy(() => import("../commands/enervate.ts"), {
	name: "enervate",
	description: "Drain/expend tass — register a withdrawal of current",
});

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
		console.log("  tassilize        crystallize quintessence into tass");
		console.log("  meditate         draw quintessence from a node");
		console.log("  enervate         drain tass");
	},
};

export async function runCli(argv: string[]): Promise<void> {
	await cli(argv, entry, {
		name: "tassle",
		version: "1.0.0",
		plugins: [completion()],
		subCommands: {
			login,
			logout,
			whoami,
			sheet,
			tassilize,
			meditate,
			enervate,
		},
	});
}
