import type { CommandRunner } from "gunshi";
import { generateAllSamples, sampleIndex } from "../samples/generate.ts";

export const run: CommandRunner = async (ctx) => {
	if (ctx.values.list) {
		for (const s of sampleIndex) {
			console.log(`  ${s.filename}`);
			console.log(`    ${s.description}`);
		}
		return;
	}

	const written = generateAllSamples();
	console.log(`✓ wrote ${written.length} samples to samples/`);
	for (const f of written) console.log(`  ${f}`);
};

export default {
	name: "samples",
	description: "Generate example records into samples/ using the fluent builders",
	args: {
		list: {
			type: "boolean",
			short: "l",
			description: "List sample files without regenerating",
			default: false,
		},
	},
	run,
};
