import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

// The ⌥ peek (components/value/altPeek.ts) keeps module state across tests: release ⌥ and forget the hovered element.
afterEach(() => {
  cleanup();
  window.dispatchEvent(new Event("blur"));
  document.body.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
});

// The core install (scopes and settings pages). Stage is not installed here; its tests call `installStage()`.
// Imported lazily so a test file's `vi.mock("../api")` is already in place when the store loads.
beforeEach(async () => {
  const { registerCoreScopes } = await import("./commands");
  const { registerCoreSettingsPages } = await import("./components/settings/pages");
  registerCoreScopes();
  registerCoreSettingsPages();
});

// jsdom lacks these browser APIs; CodeMirror and the virtualized grid touch them.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;

window.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
  dispatchEvent: () => false,
})) as typeof window.matchMedia;

const rect = { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON: () => ({}) };
Range.prototype.getClientRects = () => ({ length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator] });
Range.prototype.getBoundingClientRect = () => rect;

Object.defineProperty(navigator, "clipboard", {
  configurable: true,
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
});

// The app runs on macOS only: make CodeMirror map Mod to ⌘.
Object.defineProperty(navigator, "platform", { configurable: true, value: "MacIntel" });

// jsdom has no layout; give elements a viewport-like size so virtualized lists render rows.
Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, get: () => 800 });
Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get: () => 1200 });
