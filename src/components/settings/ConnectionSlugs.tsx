/** The slug of every saved connection: the name an address starts with (`databee://<slug>/…`). */
import { useEffect, useState } from "react";
import { api, type RecentConnection } from "../../api";
import { fail, useStore } from "../../state/store";
import { Group, TextField } from "./controls";

const title = (recent: RecentConnection) => recent.spec.name ?? `${recent.spec.database ?? "no database"} on ${recent.account}`;

export function ConnectionSlugs() {
  const [recents, setRecents] = useState<RecentConnection[]>([]);
  // Bumped after a rejected rename, so the fields show the stored slugs again.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    api.listRecents().then(setRecents, (e) => fail(e, "Could not list saved connections"));
  }, []);

  const rename = async (recent: RecentConnection, slug: string) => {
    try {
      await useStore.getState().renameConnectionSlug(recent.identity, slug);
      setRecents((list) => list.map((r) => (r.identity === recent.identity ? { ...r, slug } : r)));
    } catch (e) {
      fail(e, "Could not rename the connection");
      setAttempt((n) => n + 1);
    }
  };

  if (recents.length === 0) return null;
  return (
    <Group title="Addresses">
      {recents.map((r) => (
        <TextField
          key={`${r.identity}:${attempt}`}
          label={title(r)}
          description={`databee://${r.slug}/… · a-z, 0-9 and hyphens, unique`}
          value={r.slug}
          stage={`settings:slug:${r.identity}`}
          onCommit={(slug) => void rename(r, slug.trim())}
        />
      ))}
    </Group>
  );
}
