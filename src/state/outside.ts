/** Opening what another process names (`databee open`): failures throw, so the sender learns them. */
import { api } from "../api";
import { parseAddress, SCHEME } from "../lib/address";
import { noConnectionNamed, selectActiveSession, selectSessionBySlug } from "./selectors";
import type { Store } from "./store";

/**
 * Opens `text`: an address, or an entity shorthand (`order 4711`) in the session of `connection`
 * (a slug), else in the active session. Rust resolves the shorthand to its row.
 */
export async function openFromOutside(get: () => Store, text: string, connection: string | null): Promise<void> {
  if (text.startsWith(SCHEME)) {
    const { connection: slug } = parseAddress(text);
    if (!selectSessionBySlug(get(), slug)) throw new Error(noConnectionNamed(slug));
    return get().openAddress(text);
  }
  const session = connection ? selectSessionBySlug(get(), connection) : selectActiveSession(get());
  if (!session) throw new Error(connection ? noConnectionNamed(connection) : "No connection is open");
  const document = await api.document(session.id, text, null);
  get().keepDocument(session.id, document);
  get().openObject(document.address);
}
