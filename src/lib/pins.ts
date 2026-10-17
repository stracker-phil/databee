/** How a saved query shows on the readout (`Home`, the shape of `[[pinned]] home`). */
import type { Home } from "../api";

export type PinChoice = "none" | Home["show"];

export const DEFAULT_PIN_ROWS = 5;

export function pinChoice(pin: Home | null | undefined): PinChoice {
  return pin ? pin.show : "none";
}

export function pinOf(choice: PinChoice, limit: number): Home | null {
  switch (choice) {
    case "none":
      return null;
    case "rows":
      return { show: "rows", limit };
    case "count":
    case "value":
      return { show: choice };
  }
}

export function pinLabel(pin: Home | null | undefined): string {
  const choice = pinChoice(pin);
  switch (choice) {
    case "none":
      return "Pin";
    case "rows":
      return pin?.show === "rows" && pin.limit ? `Pinned · ${pin.limit} rows` : "Pinned · rows";
    case "count":
    case "value":
      return `Pinned · ${choice}`;
  }
}
