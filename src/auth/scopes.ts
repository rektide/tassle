/**
 * OAuth scopes requested by tassle.
 *
 * - `atproto` — base scope
 * - `repo:actor.rpg.stats` — read/modify the rpg.actor mage character sheet
 * - `repo:com.superbfowle.tass.*` — tass action records
 *
 * Adding a new collection? Add its scope here.
 */
export const OAUTH_SCOPES = [
	"atproto",
	"repo:actor.rpg.stats",
	"repo:com.superbfowle.tass.node",
	"repo:com.superbfowle.tass.tassilize",
	"repo:com.superbfowle.tass.meditate",
	"repo:com.superbfowle.tass.enervate",
].join(" ");
