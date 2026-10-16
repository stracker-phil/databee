/** The WordPress lens's install line: its command scopes, under the core scopes. */
import { registerCoreScopes } from "../../commands";
import { registerScope } from "../../commands/scopes";
import { isWpView, selectWpRoute, useStore } from "../../state/store";

/** Safe to call again: registering a scope replaces the one with the same id. */
export function installWp() {
  registerCoreScopes();
  registerScope({ id: "wp", parent: "global", label: "WordPress", isActive: () => isWpView(useStore.getState()) });
  registerScope({
    id: "document",
    parent: "wp",
    label: "Document",
    isActive: () => {
      const s = useStore.getState();
      return isWpView(s) && selectWpRoute(s).screen === "entity";
    },
  });
}
