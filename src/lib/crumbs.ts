import type { WpRoute } from "../state/store";

export interface Crumb {
  label: string;
  /** Clicking navigates here; the current (last) crumb has none. */
  route?: WpRoute;
}

/** Where the route sits in the hierarchy (never the navigation history). */
export function routeCrumbs(route: WpRoute): Crumb[] {
  switch (route.screen) {
    case "dashboard":
      return [{ label: "Dashboard" }];
    case "snapshots":
      return [{ label: "Snapshots & diff" }];
  }
}
