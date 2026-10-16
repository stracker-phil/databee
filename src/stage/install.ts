/** Stage's one install line: its Settings page and the page-side hook. */
import { Presentation } from "lucide-react";
import { registerSettingsPage } from "../registry/settingsPages";
import { installStageHooks } from "./hooks";
import { PresentationsPage } from "./PresentationsPage";

function registerStageSettingsPage() {
  registerSettingsPage({
    id: "presentations",
    group: "System",
    label: "Presentations",
    icon: Presentation,
    order: 10,
    intro: "Lets a presenter app such as Stage highlight parts of DataBee and click through it for a demo. Only apps on this Mac can connect.",
    Body: PresentationsPage,
  });
}

/** Call once, when the app starts; returns the listeners to remove. */
export function installStage() {
  registerStageSettingsPage();
  return installStageHooks();
}
