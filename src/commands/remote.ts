/** Answers requests from other processes (`databee open`, `databee command`) that only the page can answer. */
import { api, toAppError, type RemoteEvent, type RemoteRequest } from "../api";
import { formatAddress } from "../lib/address";
import { openFromOutside } from "../state/outside";
import { selectActiveTab, useStore } from "../state/store";
import { runMenuCommand } from ".";
import { getCommand } from "./registry";

async function handle(request: RemoteRequest): Promise<unknown> {
  switch (request.type) {
    case "open":
      return openFromOutside(useStore.getState, request.address, request.connection);
    case "command":
      if (!getCommand(request.id)) throw new Error(`No command ${request.id}`);
      if (!runMenuCommand(request.id)) throw new Error(`${request.id} is not available right now`);
      return null;
    case "current": {
      const tab = selectActiveTab(useStore.getState());
      return tab?.address ? formatAddress(tab.address) : null;
    }
  }
}

export async function answerRemote({ id, request }: RemoteEvent): Promise<void> {
  let result: unknown = null;
  let error: string | null = null;
  try {
    result = (await handle(request)) ?? null;
  } catch (e) {
    error = toAppError(e).message;
  }
  await api.remoteReply(id, result, error).catch((e) => api.logError(`Remote reply failed: ${toAppError(e).message}`));
}
