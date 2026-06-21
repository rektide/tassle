import { createServer, type Server } from "node:http";
import { Agent } from "@atproto/api";
import { loopbackProfile } from "./profile.ts";
import { createOAuthClient } from "./client.ts";
import {
	loadAuthInfo,
	saveAuthInfo,
	clearAuthInfo,
	fileSessionStore,
	type AuthInfo,
} from "./stores/file-store.ts";

/**
 * Bind to a free port on loopback by asking the OS for port 0.
 */
function findFreePort(): Promise<number> {
	return new Promise((resolve, reject) => {
		const srv = createServer();
		srv.listen(0, "127.0.0.1", () => {
			const addr = srv.address();
			if (addr && typeof addr === "object") {
				const port = addr.port;
				srv.close(() => resolve(port));
			} else {
				srv.close(() => reject(new Error("could not determine port")));
			}
		});
		srv.on("error", reject);
	});
}

async function resolveDIDDocument(
	did: string,
): Promise<{ serviceEndpoint: string } | null> {
	try {
		if (did.startsWith("did:plc:")) {
			const res = await fetch(`https://plc.directory/${did}`);
			if (!res.ok) return null;
			const doc = (await res.json()) as {
				service?: Array<{
					id: string;
					type: string;
					serviceEndpoint: string;
				}>;
			};
			const pds = doc.service?.find(
				(s) =>
					s.id === "#atproto_pds" ||
					s.type === "AtprotoPersonalDataServer",
			);
			return pds ? { serviceEndpoint: pds.serviceEndpoint } : null;
		}
		if (did.startsWith("did:web:")) {
			const host = did.slice("did:web:".length).replaceAll(":", "/");
			const res = await fetch(`https://${host}/.well-known/did.json`);
			if (!res.ok) return null;
			const doc = (await res.json()) as {
				service?: Array<{
					id: string;
					type: string;
					serviceEndpoint: string;
				}>;
			};
			const pds = doc.service?.find(
				(s) =>
					s.id === "#atproto_pds" ||
					s.type === "AtprotoPersonalDataServer",
			);
			return pds ? { serviceEndpoint: pds.serviceEndpoint } : null;
		}
	} catch {
		// fall through
	}
	return null;
}

/**
 * Run the full loopback OAuth login flow:
 *   1. find free port
 *   2. construct loopback client_id (must reuse port on restore)
 *   3. build authorize URL, open browser
 *   4. receive code on loopback HTTP server
 *   5. exchange for tokens, persist session
 *
 * CLI-only. The web flow is structurally simpler (no loopback server,
 * browser already in context) and lives in a hedystia route handler.
 */
export async function login(
	handle: string,
): Promise<{ did: string; handle: string }> {
	const port = await findFreePort();
	const profile = loopbackProfile(port);
	const client = createOAuthClient(profile);

	const authUrl = await client.authorize(handle, { scope: profile.metadata.scope });

	return new Promise((resolve, reject) => {
		const timeout = setTimeout(() => {
			server.close();
			reject(new Error("login timed out after 120 seconds"));
		}, 120_000);

		const server: Server = createServer(async (req, res) => {
			if (!req.url?.startsWith("/callback")) {
				res.writeHead(404);
				res.end("not found");
				return;
			}
			try {
				const url = new URL(req.url, `http://127.0.0.1:${port}`);
				const { session } = await client.callback(url.searchParams);

				const did = session.did;
				const agent = new Agent(session);
				let resolvedHandle = handle;
				try {
					const profileData = await agent.getProfile({ actor: did });
					resolvedHandle = profileData.data.handle;
				} catch {
					// keep provided handle
				}

				let service = "https://bsky.social";
				const didDoc = await resolveDIDDocument(did);
				if (didDoc) service = didDoc.serviceEndpoint;

				saveAuthInfo({ did, handle: resolvedHandle, service, oauthPort: port });

				res.writeHead(200, { "Content-Type": "text/html" });
				res.end(
					`<html><body style="font-family:system-ui;text-align:center;padding:40px">
						<h2>logged in to tassle</h2>
						<p>you can close this tab and return to your terminal.</p>
					</body></html>`,
				);

				clearTimeout(timeout);
				server.close();
				resolve({ did, handle: resolvedHandle });
			} catch (err) {
				res.writeHead(500, { "Content-Type": "text/html" });
				res.end(
					`<html><body style="font-family:system-ui;text-align:center;padding:40px">
						<h2>login failed</h2>
						<p>${err instanceof Error ? err.message : "unknown error"}</p>
					</body></html>`,
				);
				clearTimeout(timeout);
				server.close();
				reject(err instanceof Error ? err : new Error(String(err)));
			}
		});

		server.listen(port, "127.0.0.1", async () => {
			try {
				const open = (await import("open")).default;
				await open(authUrl.toString());
				console.log("\nopened browser for login. waiting for authorization...");
				console.log(
					`if the browser didn't open, visit:\n${authUrl.toString()}\n`,
				);
			} catch {
				console.log(
					`\nopen this URL in your browser to log in:\n${authUrl.toString()}\n`,
				);
			}
		});
	});
}

/**
 * Restore a previously-authenticated session by DID.
 *
 * Reuses the persisted oauthPort so the loopback client_id stays stable
 * — otherwise token refresh silently fails after the first access token
 * expires (~1 hour).
 */
export async function getAgent(
	authInfo?: AuthInfo | null,
): Promise<{ agent: Agent; did: string; handle: string } | null> {
	const info = authInfo ?? loadAuthInfo();
	if (!info) return null;
	try {
		const port = info.oauthPort ?? 0;
		const client = createOAuthClient(loopbackProfile(port));
		const session = await client.restore(info.did);
		const agent = new Agent(session);
		return { agent, did: info.did, handle: info.handle };
	} catch {
		return null;
	}
}

/**
 * Require authentication or exit with a helpful error.
 */
export async function requireAgent(): Promise<{
	agent: Agent;
	did: string;
	handle: string;
}> {
	const authInfo = loadAuthInfo();
	const result = await getAgent(authInfo);
	if (!result) {
		const stderr = authInfo
			? `session expired. run \`tassle login ${authInfo.handle}\` to re-authenticate.`
			: "not logged in. run `tassle login <handle>` first.";
		console.error(stderr);
		process.exit(1);
	}
	return result;
}

/**
 * Construct an unauthenticated Agent for public reads.
 *
 * Useful for `sheet` against public records when the user hasn't logged in
 * yet. The agent points at the given PDS URL (defaults to the dev-reference
 * PDS). Writes will fail — only public XRPC reads work.
 */
export function publicAgent(pdsUrl = "https://bsky.social"): Agent {
	// Atproto's Agent accepts a URL string for an unauthenticated session.
	// Reads of public records work; authenticated XRPCs return 401.
	return new Agent(pdsUrl);
}

export function logout(): void {
	const authInfo = loadAuthInfo();
	if (authInfo) {
		// Session is keyed by sub (DID for atproto). Best-effort delete.
		void fileSessionStore.del(authInfo.did);
	}
	clearAuthInfo();
}
