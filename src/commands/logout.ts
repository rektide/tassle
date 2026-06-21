import type { CommandRunner } from "gunshi";
import { logout } from "../auth/agent.ts";

export const run: CommandRunner = async () => {
	logout();
	console.log("✓ logged out");
};

export default {
	name: "logout",
	description: "Clear stored session",
	run,
};
