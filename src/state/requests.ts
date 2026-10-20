import { toAppError } from "../api";
import { useStore } from "./store";

/**
 * Shows an error toast (which also logs it) and returns the error. A call refused because the connection is
 * lost gets a short note instead: the toolbar already says why and offers Retry.
 */
export function fail(e: unknown, context: string) {
  const err = toAppError(e);
  if (err.kind === "connection_lost") useStore.getState().toast("info", `${context}: connection lost`);
  else useStore.getState().toast("error", `${context}: ${err.message}`);
  return err;
}
