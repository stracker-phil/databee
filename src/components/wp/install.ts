/** The WordPress lens's install line: its command scope, under the core scopes. */
import { registerCoreScopes } from "../../commands";
import { registerScope } from "../../commands/scopes";
import { isWpView, useStore } from "../../state/store";

/** Safe to call again: registering a scope replaces the one with the same id. */
export function installWp() {
  registerCoreScopes();
  registerScope({ id: "wp", parent: "global", label: "WordPress", isActive: () => isWpView(useStore.getState()) });
}
