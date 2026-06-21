import {
	mkdirSync,
	readFileSync,
	writeFileSync,
	existsSync,
	unlinkSync,
} from "node:fs";
import { join } from "node:path";
import type {
	NodeSavedSession,
	NodeSavedState,
} from "@atproto/oauth-client-node";
import {
	AUTH_DIR,
	STATE_DIR,
	CONFIG_DIR,
} from "../../config.ts";

function ensureDir(path: string, mode = 0o700): void {
	mkdirSync(path, { recursive: true, mode });
}

function writeJson(path: string, data: unknown, mode = 0o600): void {
	writeFileSync(path, JSON.stringify(data), { mode });
}

function readJson<T>(path: string): T | undefined {
	if (!existsSync(path)) return undefined;
	try {
		return JSON.parse(readFileSync(path, "utf-8")) as T;
	} catch {
		return undefined;
	}
}

function removeFile(path: string): void {
	try {
		unlinkSync(path);
	} catch {
		// ignore missing
	}
}

/**
 * File-backed state store for OAuth CSRF/PKCE state, keyed by state token.
 * Used during the authorization handshake; cleared on completion.
 */
export const fileStateStore = {
	async set(key: string, state: NodeSavedState): Promise<void> {
		ensureDir(STATE_DIR);
		writeJson(join(STATE_DIR, `${key}.json`), state);
	},
	async get(key: string): Promise<NodeSavedState | undefined> {
		return readJson<NodeSavedState>(join(STATE_DIR, `${key}.json`));
	},
	async del(key: string): Promise<void> {
		removeFile(join(STATE_DIR, `${key}.json`));
	},
};

/**
 * File-backed session store for OAuth tokens + DPoP key, keyed by user sub.
 * Persists across CLI invocations so refresh works.
 */
export const fileSessionStore = {
	async set(sub: string, session: NodeSavedSession): Promise<void> {
		ensureDir(AUTH_DIR);
		const filename = Buffer.from(sub).toString("base64url");
		writeJson(join(AUTH_DIR, `${filename}.json`), session);
	},
	async get(sub: string): Promise<NodeSavedSession | undefined> {
		const filename = Buffer.from(sub).toString("base64url");
		return readJson<NodeSavedSession>(join(AUTH_DIR, `${filename}.json`));
	},
	async del(sub: string): Promise<void> {
		const filename = Buffer.from(sub).toString("base64url");
		removeFile(join(AUTH_DIR, `${filename}.json`));
	},
};

/**
 * AuthInfo: which user is currently logged in.
 * Single record at ~/.config/tassle/session.json.
 * Not to be confused with OAuth session tokens (which live in auth/).
 */
export interface AuthInfo {
	did: string;
	handle: string;
	service: string;
	/** Port reused on restore to keep client_id stable (see profile.ts). */
	oauthPort?: number;
}

export function loadAuthInfo(): AuthInfo | null {
	ensureDir(CONFIG_DIR);
	return readJson<AuthInfo>(join(CONFIG_DIR, "session.json")) ?? null;
}

export function saveAuthInfo(info: AuthInfo): void {
	ensureDir(CONFIG_DIR);
	writeJson(join(CONFIG_DIR, "session.json"), info);
}

export function clearAuthInfo(): void {
	removeFile(join(CONFIG_DIR, "session.json"));
}
