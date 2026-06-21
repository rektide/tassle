import type { ClientMetadata } from "@atproto/oauth-client-node";
import { OAUTH_SCOPES } from "./scopes.ts";

/**
 * A ClientProfile bundles the OAuth client metadata that differs between
 * transports (loopback CLI vs public web) while keeping everything else
 * (DPoP, scopes, session shape, refresh logic) shared.
 *
 * Why split: ATProto's OAuth for native apps uses a loopback redirect URI
 * encoded directly into the client_id (no pre-registration). Web apps use a
 * public client_id served at .well-known/oauth-client-metadata. Both still
 * produce a NodeOAuthClient — only the metadata differs.
 */
export interface ClientProfile {
	name: string;
	metadata: ClientMetadata;
}

/**
 * Loopback profile for the CLI. Picks a free port, encodes it into the
 * loopback client_id per ATProto's native-app convention.
 *
 * The port MUST be reused on restore — token refresh fails if client_id
 * changes after ~1 hour when the access token expires. We persist oauthPort
 * in session.json for this reason.
 */
export function loopbackProfile(port: number): ClientProfile {
	const redirectUri = `http://127.0.0.1:${port}/callback` as const;
	const clientId = `http://localhost?redirect_uri=${encodeURIComponent(
		redirectUri,
	)}&scope=${encodeURIComponent(OAUTH_SCOPES)}` as const;
	return {
		name: "tassle-cli",
		metadata: {
			client_id: clientId,
			client_name: "Tassle CLI",
			redirect_uris: [redirectUri],
			scope: OAUTH_SCOPES,
			grant_types: ["authorization_code", "refresh_token"],
			response_types: ["code"],
			token_endpoint_auth_method: "none",
			application_type: "native",
			subject_type: "public",
			authorization_signed_response_alg: "ES256",
			dpop_bound_access_tokens: true,
		},
	};
}

/**
 * Web profile for hedystia server (used later). Requires
 * /.well-known/oauth-client-metadata served at the public origin.
 */
export function webProfile(origin: string): ClientProfile {
	// origin is runtime-determined; we can't statically verify it matches the
	// client_id/redirect_uri template literal types, so we assert.
	const redirectUri = `${origin}/callback`;
	const clientId = `${origin}/client-metadata.json`;
	return {
		name: "tassle-web",
		metadata: {
			client_id: clientId,
			client_name: "Tassle Web",
			redirect_uris: [redirectUri],
			scope: OAUTH_SCOPES,
			grant_types: ["authorization_code", "refresh_token"],
			response_types: ["code"],
			token_endpoint_auth_method: "none",
			application_type: "web",
			subject_type: "public",
			authorization_signed_response_alg: "ES256",
			dpop_bound_access_tokens: true,
		} as unknown as ClientMetadata,
	};
}
