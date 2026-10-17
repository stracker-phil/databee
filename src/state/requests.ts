import { toAppError } from "../api";
import { useStore } from "./store";

/** Shows an error toast (which also logs it) and returns the error. */
export function fail(e: unknown, context: string) {
  const err = toAppError(e);
  useStore.getState().toast("error", `${context}: ${err.message}`);
  return err;
}
