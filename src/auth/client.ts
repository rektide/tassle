import { NodeOAuthClient } from "@atproto/oauth-client-node";
import { requestLocalLock } from "@atproto/oauth-client";
import type { ClientProfile } from "./profile.ts";
import { fileStateStore, fileSessionStore } from "./stores/file-store.ts";

/**
 * Construct a NodeOAuthClient bound to the file-backed stores.
 *
 * Both CLI (loopback) and future web (hedystia/db) callers go through this
 * factory — only the ClientProfile differs. The web variant will eventually
 * pass a hedystia-backed SessionStore/StateStore instead of the file ones.
 */
export function createOAuthClient(profile: ClientProfile): NodeOAuthClient {
	return new NodeOAuthClient({
		clientMetadata: profile.metadata,
		stateStore: fileStateStore,
		sessionStore: fileSessionStore,
		requestLock: requestLocalLock,
	});
}
