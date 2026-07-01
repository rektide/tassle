# ADR 0001: Profile Config Before OAuth

## Status

Accepted.

## Context

Tassle needs a default actor before it needs full authentication. Public reads such as `repo list`, `mage list`, and `self stats` only need to know which DID/handle to inspect and which PDS hosts that actor's repo. OAuth will add tokens and write permissions later, but forcing OAuth into the first Rust CLI slice would slow down the public-read loop.

The CLI also needs multi-profile support from the start. A future user may have a player DID, a Storyteller DID, and one or more service/reality DIDs. Logging one profile in must not destroy another profile's settings.

## Decision

Add `tass auth login <did-or-handle>` now as a profile-login stub. It does not authenticate yet. It resolves the actor through Jacquard, stores DID/handle/PDS metadata, and makes that profile usable as the default repo for read commands.

Profile config lives under the XDG config directory:

```text
${XDG_CONFIG_HOME:-~/.config}/tass/config.toml.d/
├── did:plc:<profile-a>.toml
└── did:plc:<profile-b>.toml
```

Each profile is a separate TOML fragment keyed by the profile DID filename. The current fragment shape is:

```toml
id = "did:plc:..."
did = "did:plc:..."
handle = "example.bsky.social"
pds = "https://..."
active = true
created_at = "..."
updated_at = "..."
```

The writer preserves unrelated TOML content when updating an existing profile, so future profile-local settings can live in the same file. Multiple fragments may have `active = true`; the current default selection is the active profile with the newest `updated_at`.

## Rationale

- Public read commands become usable immediately without OAuth.
- Multi-profile behavior is natural: add or update one file, never rewrite a monolithic config.
- The `.d` layout matches the intended future integration with `figments-rs` directory expansion/operator semantics.
- OAuth can later extend the same profile model with token/session references instead of replacing it.

## Consequences

- `--repo` / `--actor` can default to the saved profile DID.
- `auth login` is intentionally a misnomer for now; it is profile login, not token auth. This should be made explicit in help text until OAuth lands.
- We need a later decision for active-profile semantics: one active profile globally, one active profile per workspace, or weighted defaults by command/reality.
- Sensitive OAuth tokens should not be stored directly in these public-ish profile fragments without a separate security decision.
- `auth set <key>` and `auth set <key=value>` provide a low-level editor for early profile-fragment experimentation. This uses dotted TOML paths, not CEL selectors. CEL lookup/filtering should be designed across all read/list commands rather than embedded only in auth config.

## Follow-Up

- Implement real OAuth under the same `auth login` command without destroying existing profile files.
- Integrate `figments-rs` for `.d` config expansion once the dependency is ready.
- Decide whether `active = true` should be exclusive or whether newest-active remains sufficient.
