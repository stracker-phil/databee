import { LIST_LABELS } from "../commands";
import type { EntityMeta, WpList, WpRoute } from "../state/store";

export interface Crumb {
  label: string;
  /** Clicking navigates here; the current (last) crumb has none. */
  route?: WpRoute;
}

const ENTITY_PARENT: Record<string, WpList> = {
  order: "orders",
  user: "customers",
  post: "content",
  action: "actions",
  session: "sessions",
  option: "settings",
};

export function statusLabel(status: string): string {
  const s = status.replace(/^wc-/, "").replace(/[-_]/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function listCrumb(list: WpList): Crumb {
  return { label: LIST_LABELS[list], route: { screen: "list", list } };
}

/** Where the route sits in the hierarchy (never the navigation history). */
export function routeCrumbs(route: WpRoute, meta?: Pick<EntityMeta, "parent" | "title"> | null): Crumb[] {
  switch (route.screen) {
    case "dashboard":
      return [{ label: "Dashboard" }];
    case "snapshots":
      return [{ label: "Snapshots & diff" }];
    case "find":
      return [{ label: "Find everywhere" }, { label: `“${route.value}”` }];
    case "list": {
      const crumbs = [listCrumb(route.list)];
      const status = route.params?.status;
      const kind = route.params?.kind;
      const search = route.params?.search;
      if (status) crumbs.push({ label: statusLabel(status) });
      else if (kind) crumbs.push({ label: kind });
      if (search) crumbs.push({ label: `“${search}”` });
      return crumbs;
    }
    case "entity": {
      const parent = meta?.parent ?? ENTITY_PARENT[route.entity];
      const own = { label: meta?.title ?? `#${route.id}` };
      return parent ? [listCrumb(parent), own] : [own];
    }
    case "compare": {
      const parent = ENTITY_PARENT[route.entity];
      const own = { label: `Compare #${route.a} ↔ #${route.b}` };
      return parent ? [listCrumb(parent), own] : [own];
    }
  }
}
