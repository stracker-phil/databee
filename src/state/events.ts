/** Session lifecycle events; modules subscribe instead of the core calling into them. */
export type SessionEvent =
  /** The session's current database changed or its database list was reloaded. */
  | { type: "databaseChanged"; sessionId: string; database: string }
  | { type: "sessionClosed"; sessionId: string };

type Handler = (event: SessionEvent) => void;

const handlers = new Set<Handler>();

export function onSessionEvent(handler: Handler): () => void {
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

export function emitSessionEvent(event: SessionEvent) {
  for (const handler of [...handlers]) handler(event);
}
