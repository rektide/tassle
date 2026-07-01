/**
 * OAuth scopes requested by tassle.
 *
 * - `atproto` — base scope
 * - `repo:actor.rpg.stats` — read/modify the rpg.actor mage character sheet
 * - `repo:at.telluri.*` — tass action records
 *
 * Adding a new collection? Add its scope here.
 */
export const OAUTH_SCOPES = [
	"atproto",
	"repo:actor.rpg.stats",
	"repo:at.telluri.node",
	"repo:at.telluri.act.tassilize",
	"repo:at.telluri.act.meditate",
	"repo:at.telluri.act.enervate",
].join(" ");
