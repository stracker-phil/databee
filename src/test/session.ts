import type { SessionInfo } from "../api/types";

export const session: SessionInfo = {
  id: "s1",
  spec: { name: "Shop", host: "127.0.0.1", port: 52958, user: "root", database: "db" },
  identity: "root@127.0.0.1:52958/db",
  account: "root@127.0.0.1:52958",
  displayName: "Shop",
  serverVersion: "10.3.39-MariaDB",
  database: "db",
  slug: "shop",
};
