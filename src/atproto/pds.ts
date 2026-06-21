import type { Agent } from "@atproto/api";
import { TASS_COLLECTIONS, type TassCollection } from "./tass.ts";

/**
 * List records of a given tass collection owned by `did`.
 * Returns at-uris + raw values.
 */
export async function listTassRecords(
	agent: Agent,
	did: string,
	collection: TassCollection,
	opts: { limit?: number; cursor?: string } = {},
): Promise<{
	records: Array<{ uri: string; value: Record<string, unknown> }>;
	cursor?: string;
}> {
	const res = await agent.com.atproto.repo.listRecords({
		repo: did,
		collection,
		limit: opts.limit ?? 50,
		cursor: opts.cursor,
	});
	return {
		records: res.data.records.map((r) => ({
			uri: r.uri,
			value: r.value as Record<string, unknown>,
		})),
		cursor: res.data.cursor,
	};
}

/**
 * Publish a record under one of the tass collections.
 * Generates a TID rkey if not supplied.
 */
export async function putTassRecord(
	agent: Agent,
	did: string,
	collection: TassCollection,
	value: Record<string, unknown>,
	opts: { rkey?: string; validate?: boolean } = {},
): Promise<{ uri: string; cid: string }> {
	const res = await agent.com.atproto.repo.putRecord({
		repo: did,
		collection,
		rkey: opts.rkey ?? (await import("@atproto/common-web")).TID.nextStr(),
		record: value,
		validate: opts.validate ?? false,
	});
	void TASS_COLLECTIONS; // referenced for type narrowing
	return { uri: res.data.uri, cid: res.data.cid ?? "" };
}
