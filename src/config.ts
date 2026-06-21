import { join } from "node:path";
import { homedir } from "node:os";

/**
 * Dev-time reference to rektide's rpg.actor character sheet.
 * Used as a default for local testing — production users supply their own handle.
 */
export const DEV_REFERENCE = {
	handle: "jauntywk.bsky.social",
	did: "did:plc:zjbq26wybii5ojoypkso2mso",
	pdsEndpoint: "https://puffball.us-east.host.bsky.network",
	statsCollection: "actor.rpg.stats",
	statsRkey: "self",
	mageKey: "mage",
} as const;

export const CONFIG_DIR = join(homedir(), ".config", "tassle");
export const CONFIG_PATH = join(CONFIG_DIR, "config.json");
export const AUTH_DIR = join(CONFIG_DIR, "auth");
export const STATE_DIR = join(CONFIG_DIR, "state");
export const AUTH_INFO_PATH = join(CONFIG_DIR, "session.json");
