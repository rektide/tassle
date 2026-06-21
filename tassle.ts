#!/usr/bin/env node
import { realpath } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const isMain = (await realpath(process.argv[1] ?? "").catch(() => "")) ===
	(await realpath(new URL(import.meta.url).pathname).catch(() => ""));

if (isMain) {
	const { runCli } = await import("./src/cli/main.ts");
	await runCli(process.argv.slice(2));
}
