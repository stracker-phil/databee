import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { applyInitialScheme } from "./lib/theme";
import { installModules } from "./modules";
import { useStore } from "./state/store";

installModules();
applyInitialScheme();

// Lets scripts/dev/js.sh drive the debug build; release builds don't hand the whole store to the page.
if (import.meta.env.VITE_DEV_HOOKS === "1") window.databee = useStore;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
