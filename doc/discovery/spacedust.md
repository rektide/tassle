# Spacedust: commands-by-text as tassle's primary interface

> How tassle uses [Spacedust](https://www.microcosm.blue/) — microcosm's "configurable ATProto notifications firehose" — as its **primary user interface**: a person posts something like *"burn my tass"* at our account, and we react by reading their records, performing the action, and posting an attestation that we did it. This is a sibling to the [attestation options](attestation.md) discovery doc (which covers *cosign* trust) and feeds the [`tass-listener-svc`](#how-this-fits-the-existing-plan) epic. Source studied: `~/archive/microcosm.blue/microcosm-rs/{spacedust,links,jetstream}`.

---

## 1. The idea

Tassle's primary interface is **not** a CLI or a web form — it is **atproto posts addressed at us**. A user writes, in plain text, a command like:

> "@mage burn my tass"

and the tassle service, running as a dedicated **Mage account**, reacts:

1. **Watches** for posts that reference us (mention / reply).
2. **Hydrates** the post to read the command text.
3. **Looks up the user's records** (their `tassilize` tass, their `node`).
4. **Does the thing** — writes the corresponding action record in *our* repo.
5. **Attests** — posts an attestation record in our own NSID saying what we did.

Two flows are in scope for MVP **v1.1**:

| Command (example phrasing) | Action record written | Effect |
| --- | --- | --- |
| **"burn my tass"** (primary) | `com.superbfowle.tass.enervate` | Drains the user's tass; the quintessence flows to the Mage. |
| **"meditate"** (second, MVP-final) | `com.superbfowle.tass.meditate` | The Mage pulls ambient quintessence from a Node. |

Everything else (tassilize, resonance, forms, cosigns) is out of scope for v1.1.

Spacedust is the right substrate for step 1 because, as its server banner says, it is *"A configurable ATProto notifications firehose"* (`spacedust/src/server.rs:59`). It does the "who is talking at us" filtering **server-side, by our DID**, so we receive only the handful of posts actually addressed to us — not the entire global post firehose (which is what a raw Jetstream / Hydrant consumer would force us to inspect record-by-record).

---

## 2. What Spacedust is, mechanically

A WebSocket service with one real endpoint: `GET /subscribe` (upgraded to a WS channel — `server.rs:317`). Internally it consumes Jetstream, runs every record through `collect_links` (which walks the record JSON and emits one event per link/reference it finds — `consumer.rs:89`, `links/src/record.rs:walk_record`), and fans those link-events out to subscribers who filter for the ones they care about.

The framing that makes this work: **all social interactions in atproto are links.** A reply, a mention, a quote, a like, a repost, a follow — each is just a record whose *link target* contains some DID or at-uri. "A post at us" = "a link whose target carries our DID."

---

## 3. The `/subscribe` filters (the "at us" mechanism)

Filters are supplied as query params on the subscribe URL, and can also be **updated live** by sending a JSON text frame on the open socket (`subscriber.rs:77`, `subscriber.rs:154`).

### Wanted-params table

| Param | What it matches | Limit | Tassle use |
| --- | --- | --- | --- |
| `wantedSubjectDids` | DIDs to receive links about — matches **bare-DID links _and_ DIDs extracted from at-uri targets** (`lib.rs:40`) | 10 000 | **Our primary filter.** `wantedSubjectDids=<MAGE_DID>` catches *everything aimed at us*: replies, quotes, mentions, likes, follows. |
| `wantedSubjectPrefixes` | Prefix match on the target | 100 | `at://<MAGE_DID>/` to catch links to *any of our records specifically* (e.g. someone enervating one of our tass). Or `at://<MAGE_DID>/app.bsky.feed.post/` to scope to replies to our posts only. |
| `wantedSubjects` | Exact target at-uri / uri / DID | 50 000 | Watch one specific record — e.g. a single "command thread" post we published, where every reply is a command. |
| `wantedSources` | Link **source** = `<collection NSID>:<dotted record path>` (`lib.rs:48`) | 1 000 | Narrow to *textual commands only* (posts), filtering out likes/follows/etc. See below. |

### Filter logic

From `subscriber.rs:123`:

- `wantedSubjects` **OR** `wantedSubjectPrefixes` **OR** `wantedSubjectDids` (the subject group is a logical OR).
- That whole group is **AND**'d with `wantedSources`.
- An empty group matches everything.

So **"posts that mention or reply to us"** =

```
wantedSubjectDids = <MAGE_DID>
        AND
wantedSources    = { "app.bsky.feed.post:reply.parent.uri",
                     "app.bsky.feed.post:facets[].features[app.bsky.richtext.facet#mention].did" }
```

- `app.bsky.feed.post:reply.parent.uri` — a reply to one of our posts (the parent at-uri carries our DID). (Reply paths confirmed by the test at `links/src/record.rs:65`.)
- `app.bsky.feed.post:facets[].features[app.bsky.richtext.facet#mention].did` — an @-mention of us (a richtext facet whose `did` feature is our bare DID). The typed-array path notation (`[app.bsky.richtext.facet#mention]`) comes from `walk_record` keying array entries on their `$type` (`links/src/record.rs:14`).

This `wantedSources` AND-clause is what keeps the firehose from drowning us in likes/reposts when all we want is text we can parse.

### The `instant` flag (scalar param)

By default Spacedust **holds every link for 21 seconds** before emitting, so an interaction that gets undone quickly (≈<1% of links) never fires a notification (`server.rs:299`). `instant=true` bypasses the delay buffer for faster, noisier delivery. For a command interface we want the **default (delayed)** stream — a user who posts and immediately deletes a "burn my tass" should not trigger a burn.

---

## 4. What an event contains — and the hydration step

The payload is a **pointer, not the content** (`lib.rs:89`, `ClientLinkEvent`):

```json
{ "kind": "link", "origin": "live",
  "link": {
    "operation": "create",
    "source":        "app.bsky.feed.post:reply.parent.uri",
    "source_record": "at://did:plc:THEM/app.bsky.feed.post/3l...",   // the post that mentioned us
    "source_rev":    "...",
    "subject":       "at://did:plc:MAGE/app.bsky.feed.post/3k..."     // our record they referenced
  } }
```

There is deliberately **no record body** (`lib.rs:96`). So to read the actual command text we must **hydrate `source_record`** — fetch the referenced post. This is the one place [Slingshot](https://www.microcosm.blue/) earns its keep (it is a read-side record cache, useless as a listener but good here): `getRecordByUri?at_uri=<source_record>` returns `{uri, cid, value}`. So the two microcosm tools compose — **Spacedust targets, Slingshot hydrates** — with a plain PDS `getRecord` as the fallback.

---

## 5. The tassle command pipeline

```
spacedust /subscribe?wantedSubjectDids=<MAGE_DID>&wantedSources=<post mention/reply>   (1) WATCH
   → hydrate source_record  (Slingshot getRecordByUri, PDS fallback)                   (2) READ COMMAND
   → parse command grammar  ("burn my tass" | "meditate")                              (3) PARSE
   → resolve the user's records  (their tassilize / node)                              (4) LOOK FOR RECORDS
   → perform action  (write enervate / meditate in the MAGE repo)                      (5) DO THE THING
   → post attestation record in com.superbfowle.tass.*  (what we changed, from → to)   (6) ATTEST
   → on reconnect, backfill the gap from Constellation, dedupe by source_record+rev    (recovery)
```

### (4)–(5): looking for the records, and doing the things

Spacedust gives us *both halves* of what we need: it tells us a post is at us **and** it surfaces the at-uris in that post, so we can both **find the user's records** and **act on them**.

- **"burn my tass" → enervate (primary).** We act *as the Mage*: we write a `com.superbfowle.tass.enervate` record **in our own (Mage) repo** whose `tass` field is the at-uri of the user's `com.superbfowle.tass.tassilize` record, with `amount` = the quintessence drained (`enervate.json`: `tass` at-uri, `amount` 0–100, `purpose`, `createdAt`). The enervate reduces the available quintessence on the referenced tass, and — because the Mage authored it — that current "flows to the Mage." (Note: the enervate lexicon has **no explicit recipient field**; "to the Mage" is currently modeled only by *who authored the record*. See open questions.)
- **"meditate" → meditate (MVP v1.1 second flow).** The Mage pulls ambient quintessence from a Node: a `com.superbfowle.tass.meditate` record (`meditate.json`: `node` at-uri, `amount` 0–20, `createdAt`). Limited by Avatar rating out-of-band (the lexicon does not enforce it).

We can only write to **our own** repo, never the user's PDS. "Create records for people" therefore means: we create records *in the Mage repo* that reference and act on the user's records — not edits to their repo.

### (6): the attestation output

After doing the thing, we post an **attestation record in our own NSID** (e.g. `com.superbfowle.tass.*`) that says *we did it* and records the state change. Concretely it should reference the action record we wrote and the user record we acted on, and capture the **before → after** of the affected quantity (the tass's `quintessence` before and after the enervate, or the Mage's pattern total before/after a meditate).

> **Status: the attestation shape is still "meh."** The [attestation discovery doc](attestation.md) covers *cosign* trust (keytrace/co-core/atproto-attestation) — a different concern from this lightweight **action receipt**. The receipt idea here is deliberately small: a per-action record proving the Mage performed a state change. One concrete improvement is already ticketed: **carry an AGE-encrypted payload on the attestation describing exactly what changed, from → to** — so the before/after is verifiable by the affected user (who holds the key) without publishing the raw ledger numbers to the world. See beads `tass-1bq`.

---

## 6. Config: the account we listen from

The listener needs a configured **specific account to listen from / as**:

- `MAGE_DID` (or handle, resolved to DID) — the account commands are addressed *at*; this is the value we pass as `wantedSubjectDids`, and the repo we write action + attestation records *into*.
- Spacedust endpoint — microcosm's public instance (expected `wss://spacedust.microcosm.blue/subscribe`, following the `*.microcosm.blue` convention of `constellation.`/`slingshot.`/`ufos.`; **confirm the host** before relying on it), with a self-hosted instance as a fallback.
- Hydration endpoint — a Slingshot base URL, PDS fallback.
- A persisted **cursor / dedupe set** — see limitations.

This slots into the existing profile/config layering (`doc/adr/0001-profile-config-before-auth.md`); the Mage account is just another configured identity, and writes use the authenticated write path being built under `tass-quint-auth-config` / `tass-refresh`.

---

## 7. Limitations to design around

Spacedust's current version is intentionally lightweight (its readme):

1. **No replay window, no delete events.** If the listener disconnects, everything in the gap is lost; and once a link is emitted, no later delete event is sent. → On reconnect, **backfill from [Constellation](https://constellation.microcosm.blue/)** (the backlink *index*): `GET /links?target=<MAGE_DID>&collection=app.bsky.feed.post&path=.reply.parent.uri` (and the mention path), then **dedupe by `source_record` + `source_rev`** against what we have already processed. This also protects against double-acting on a command after a restart.
2. **Slow-consumer drop.** A subscriber that can't keep up is dropped as a laggard (`subscriber.rs:52`). Keep the socket handler cheap: enqueue events and do hydration / action / attestation off-thread.
3. **Idempotency is on us.** A command should produce exactly one enervate even if we see the link twice (live + backfill). Key idempotency on `(source_record, source_rev)` and refuse to re-run a command we have an attestation for.

A heavier next-gen Spacedust (full forward-link index, replay, hydrated deletes) is planned upstream; if/when it lands, items 1 and 3 get easier.

---

## 8. How this fits the existing plan

Spacedust is **another `EventSource` impl under [`tass-sync-source`](../../)** — but a different *shape* than the Jetstream/Hydrant sources: it yields **link notifications, not record bodies**, so it needs a hydration adapter (`NotificationSource → hydrate → RecordEvent`) before the [`tass-listener-svc`](../../) fold. The command pipeline (parse → act → attest) is new surface that sits on top of that source, gated behind the same listener cargo feature as the Hydrant path.

Relationship to the alternatives studied earlier:

- **Jetstream / Hydrant** (full bodies, no "aimed-at-me" filter): you ingest the whole post firehose and inspect every record. Right for *indexing all tass activity*; wrong for *a command inbox*.
- **Spacedust** (server-side target-by-DID filter, pointers only): you receive only what is addressed to you, at the cost of one hydration round-trip. **Right for the command interface.**
- **Constellation** (backlink index, query not stream): the catch-up / dedupe companion for Spacedust's missing replay.
- **Slingshot** (record cache, query not stream): the hydration companion for Spacedust's body-less payloads.

A complete v1.1 listener uses **three** microcosm services together: Spacedust to notice, Slingshot to read, Constellation to recover.

---

## 9. Open questions

1. **Recipient modeling.** "Burn my tass *to the Mage*" is currently implicit in record authorship. Should `enervate` (or a wrapping record) gain an explicit recipient field, or is author-is-recipient sufficient for v1.1?
2. **Command grammar.** What exactly do we parse? Just `"burn my tass"` / `"meditate"` keyword-spotting for v1.1, or a small structured grammar (amounts, target selection when a user has multiple tass)? How do we disambiguate *which* tass when the user has several?
3. **Authorization.** Anyone can post "burn my tass" at us. Do we only act on commands from accounts in some allowlist / with a sheet, or is acting-on-anyone fine because the action only drains *their own* tass?
4. **Confirm the Spacedust public host** (`spacedust.microcosm.blue`?) and decide self-host vs. public for a load-bearing primary interface (best-effort uptime upstream — a pi4).
5. **Attestation payload.** Lands in beads `tass-1bq`: AGE-encrypted before→after on the attestation record. Who holds the decryption key (the affected user? the reality?), and what's the recipient/key-discovery story?

## See also

- [attestation.md](attestation.md) — cosign *trust* options (keytrace / co-core / atproto-attestation); distinct from this doc's lightweight action-receipt attestation.
- [ledger.md](../ledger.md), [hedystia-listener-design.md](../hedystia-listener-design.md) — the fold/ledger and listener-service designs this pipeline feeds.
- `~/archive/microcosm.blue/microcosm-rs/spacedust/` — `server.rs` (subscribe + params), `subscriber.rs` (filter logic), `consumer.rs` + `links/src/record.rs` (link extraction), `lib.rs` (event payload).
