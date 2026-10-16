import { toAppError } from "../api";
import { useStore } from "./store";

/** Shows an error toast (which also logs it) and returns the error. */
export function fail(e: unknown, context: string) {
  const err = toAppError(e);
  useStore.getState().toast("error", `${context}: ${err.message}`);
  return err;
}

/** Request tokens: only the newest request per key may write its reply. */
const latestRequest = new Map<string, number>();
let nextRequestToken = 1;

export function beginRequest(key: string) {
  const token = nextRequestToken++;
  latestRequest.set(key, token);
  return token;
}

export const isLatest = (key: string, token: number) => latestRequest.get(key) === token;
