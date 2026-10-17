/** Opening what another process names (`databee open`): failures throw, so the sender learns them. */
import { api } from "../api";
import { parseAddress, SCHEME } from "../lib/address";
import { selectActiveSession } from "./selectors";
import type { Store } from "./store";

const noConnectionNamed = (slug: string) => new Error(`No open connection is named ${slug}`);

/**
 * Opens `text`: an address, or an entity shorthand (`order 4711`) in the session of `connection`
 * (a slug), else in the active session. Rust resolves the shorthand to its row.
 */
export async function openFromOutside(get: () => Store, text: string, connection: string | null): Promise<void> {
  if (text.startsWith(SCHEME)) {
    const { connection: slug } = parseAddress(text);
    if (!get().sessions.some((x) => x.slug === slug)) throw noConnectionNamed(slug);
    return get().openAddress(text);
  }
  const session = connection ? get().sessions.find((x) => x.slug === connection) : selectActiveSession(get());
  if (!session) throw connection ? noConnectionNamed(connection) : new Error("No connection is open");
  const document = await api.document(session.id, text, null);
  get().keepDocument(session.id, document);
  get().openObject(document.address);
}
