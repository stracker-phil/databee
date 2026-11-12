import type { AppWindow } from "../modules";
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
    keywords: ["stage", "demo"],
    rows: [
      { id: "presentations-api", title: "Presentation API" },
      { id: "presentations-allow", title: "Allow presentations" },
      { id: "presentations-status", title: "Status" },
      { id: "presentations-port", title: "Port" },
      { id: "presentations-token", title: "Token" },
      { id: "presentations-register", title: "Register with Stage", under: "presentations-api" },
      { id: "presentations-last-request", title: "Last presenter request" },
    ],
    Body: PresentationsPage,
  });
}

/** Call once, when a window starts; only the main window answers Stage. */
export function installStage(window: AppWindow) {
  registerStageSettingsPage();
  if (window === "main") void Promise.all(installStageHooks());
}
