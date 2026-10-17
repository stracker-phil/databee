/**
 * The frontend's install list: the core first, then one line per module. Removing a module is deleting
 * its folder and its line here; the core never imports a module.
 */
import { installCommands } from "./commands";
import { registerCoreSettingsPages } from "./components/settings/pages";
import { installStage } from "./stage/install";

/** Call once, before the first render. Stage's event listeners live for the app's lifetime. */
export function installModules() {
  registerCoreSettingsPages();
  installStage();
  installCommands();
}
