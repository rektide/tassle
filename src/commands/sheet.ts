import type { CommandRunner } from "gunshi";
import { publicAgent } from "../auth/agent.ts";
import { loadAuthInfo } from "../auth/stores/file-store.ts";
import { DEV_REFERENCE } from "../config.ts";
import {
	fetchMageSheet,
	SPHERES,
	type Sphere,
} from "../atproto/mage-sheet.ts";

const SPHERE_GLYPHS: Record<Sphere, string> = {
	correspondence: "⬔",
	entropy: "♾",
	forces: "⚡",
	life: "❤",
	matter: "▼",
	mind: "✦",
	prime: "✺",
	spirit: "☄",
	time: "⧗",
};

function bar(n: number | undefined, max: number): string {
	if (n === undefined) return "—".padEnd(max, "—");
	const filled = Math.min(n, max);
	return "█".repeat(filled) + "░".repeat(Math.max(0, max - filled));
}

function renderSheet(
	sheet: Awaited<ReturnType<typeof fetchMageSheet>>,
): string {
	if (!sheet) return "no mage sheet found";
	const lines: string[] = [];
	lines.push("═══ Mage: The Ascension ═══");
	if (sheet.updatedAt) {
		lines.push(`  updated: ${sheet.updatedAt}`);
	}
	lines.push("");
	lines.push("─── Advantages ───");
	lines.push(
		`  Arete       ${bar(sheet.arete, 10)} ${sheet.arete ?? "—"} / 10`,
	);
	const wp = sheet.willpower;
	const wpCurrent = typeof wp === "number" ? wp : wp?.current;
	const wpMax = typeof wp === "number" ? wp : wp?.max;
	lines.push(
		`  Willpower   ${bar(wpCurrent, wpMax ?? 10)} ${wpCurrent ?? "—"}${wpMax ? ` / ${wpMax}` : ""}`,
	);
	lines.push(
		`  Quintessence ${bar(sheet.quintessence, 20)} ${sheet.quintessence ?? "—"} / 20`,
	);
	lines.push(
		`  Paradox     ${bar(sheet.paradox, 20)} ${sheet.paradox ?? "—"} / 20`,
	);
	lines.push("");
	lines.push("─── Spheres ───");
	for (const s of SPHERES) {
		const v = sheet.spheres[s];
		const glyph = SPHERE_GLYPHS[s];
		const name = s.padEnd(15);
		lines.push(`  ${glyph} ${name} ${bar(v, 5)} ${v ?? "—"}`);
	}
	return lines.join("\n");
}

export const run: CommandRunner = async (ctx) => {
	const didArg = ctx.values.did as string | undefined;
	const rkey = ctx.values.rkey as string | undefined;

	// Try authenticated first; fall back to a public agent for public reads
	// (the dev-reference sheet and most user sheets are public).
	const auth = await (async () => {
		const info = loadAuthInfo();
		if (!info) return null;
		const { getAgent } = await import("../auth/agent.ts");
		return await getAgent(info);
	})();

	const agent = auth?.agent ?? publicAgent(DEV_REFERENCE.pdsEndpoint);
	const targetDid = didArg ?? auth?.did ?? DEV_REFERENCE.did;

	const sheet = await fetchMageSheet(agent, { did: targetDid, rkey });
	if (!sheet) {
		console.error("no mage block found in actor.rpg.stats record");
		process.exit(1);
	}
	if (ctx.values.json) {
		console.log(JSON.stringify(sheet.raw, null, 2));
	} else {
		console.log(renderSheet(sheet));
	}
};

export default {
	name: "sheet",
	description: "Read your Mage: The Ascension character sheet",
	args: {
		did: {
			type: "string",
			short: "d",
			description: "Override DID (default: logged-in user)",
		},
		rkey: {
			type: "string",
			short: "r",
			description: "Override rkey (default: self)",
		},
		json: {
			type: "boolean",
			short: "j",
			description: "Output raw record JSON",
			default: false,
		},
	},
	run,
};
