import { LIST_LABELS } from "../commands";
import type { WpList, WpRoute } from "../state/store";

export interface Crumb {
  label: string;
  /** Clicking navigates here; the current (last) crumb has none. */
  route?: WpRoute;
}

export function statusLabel(status: string): string {
  const s = status.replace(/^wc-/, "").replace(/[-_]/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function listCrumb(list: WpList): Crumb {
  return { label: LIST_LABELS[list], route: { screen: "list", list } };
}

/** Where the route sits in the hierarchy (never the navigation history). */
export function routeCrumbs(route: WpRoute): Crumb[] {
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
      if (route.params?.name) crumbs.push({ label: route.params.name });
      else if (status) crumbs.push({ label: statusLabel(status) });
      else if (kind) crumbs.push({ label: kind });
      if (search) crumbs.push({ label: `“${search}”` });
      return crumbs;
    }
  }
}
