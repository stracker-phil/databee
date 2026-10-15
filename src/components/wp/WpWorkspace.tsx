import { ArrowLeft, ArrowRight, Boxes, CalendarClock, ChevronRight, FileText, LayoutDashboard, Repeat, Search, Settings2, ShoppingBag, ShoppingCart, Users } from "lucide-react";
import { api, type BrowserSection } from "../../api";
import { routeCrumbs } from "../../lib/crumbs";
import { selectWp, selectWpRoute, useStore, type WpList, type WpRoute } from "../../state/store";
import { ErrorPanel } from "../QueryTab";
import { DashboardScreen } from "./Dashboard";
import { ListScreen } from "./Lists";
import { EntityScreen } from "./views/EntityScreen";
import { SettingsScreen } from "./Settings";
import { CompareScreen, SnapshotsScreen } from "./Debugging";
import { useWpData } from "./useWpData";

const ICONS: Record<string, typeof ShoppingCart> = {
  orders: ShoppingCart,
  subscriptions: Repeat,
  customers: Users,
  products: ShoppingBag,
  actions: CalendarClock,
  sessions: Boxes,
  content: FileText,
  settings: Settings2,
};

/** Statuses that need a look; everything else is just a number. */
const ATTENTION: Record<string, { statuses: string[]; tone: "red" | "orange"; label: string }[]> = {
  orders: [
    { statuses: ["wc-failed"], tone: "red", label: "failed" },
    { statuses: ["wc-on-hold"], tone: "orange", label: "on hold" },
  ],
  subscriptions: [{ statuses: ["wc-on-hold", "wc-pending-cancel"], tone: "orange", label: "on hold" }],
  actions: [{ statuses: ["failed"], tone: "red", label: "failed" }],
  sessions: [{ statuses: ["expired"], tone: "orange", label: "expired" }],
};

export function attentionBadges(section: BrowserSection): { count: number; tone: "red" | "orange"; label: string }[] {
  return (ATTENTION[section.list] ?? [])
    .map((rule) => ({
      ...rule,
      count: section.breakdown.filter((f) => rule.statuses.includes(f.value)).reduce((s, f) => s + f.count, 0),
    }))
    .filter((b) => b.count > 0)
    .map(({ count, tone, label }) => ({ count, tone, label }));
}

export function routeTitle(route: WpRoute): string {
  switch (route.screen) {
    case "dashboard":
      return "Dashboard";
    case "list":
      return route.list.charAt(0).toUpperCase() + route.list.slice(1);
    case "entity":
      return `${route.entity.charAt(0).toUpperCase() + route.entity.slice(1)} #${route.id}`;
    case "compare":
      return `Compare #${route.a} ↔ #${route.b}`;
    case "snapshots":
      return "Snapshots";
  }
}

function NavItem({ section, active }: { section: BrowserSection; active: boolean }) {
  const navigate = useStore((s) => s.navigate);
  const Icon = ICONS[section.list] ?? FileText;
  // Only the most severe badge fits the row; the rest are in the tooltip.
  const badges = attentionBadges(section);
  return (
    <button className={`nav-item${active ? " active" : ""}`} onClick={() => navigate({ screen: "list", list: section.list as WpList })}>
      <span className="nav-row">
        <Icon size="1.077rem" />
        <span className="nav-label">{section.label}</span>
        {badges.length > 0 && (
          <span className={`nav-badge tone-${badges[0].tone}`} title={badges.map((b) => `${b.count.toLocaleString("en-US")} ${b.label}`).join(" · ")}>
            {badges[0].count.toLocaleString("en-US")} {badges[0].label}
            {badges.length > 1 && ` +${badges.length - 1}`}
          </span>
        )}
        <span className="nav-count">{section.count.toLocaleString("en-US")}</span>
      </span>
    </button>
  );
}

function Nav({ route }: { route: WpRoute }) {
  const navigate = useStore((s) => s.navigate);
  const { data } = useWpData(api.wpBrowser, []);
  const activeList = route.screen === "list" ? route.list : null;
  return (
    <nav className="wp-nav" aria-label="WordPress">
      <button className={`nav-item${route.screen === "dashboard" ? " active" : ""}`} onClick={() => navigate({ screen: "dashboard" })}>
        <span className="nav-row">
          <LayoutDashboard size="1.077rem" />
          <span className="nav-label">Dashboard</span>
        </span>
      </button>
      {(data ?? []).map((s) => (
        <NavItem key={s.list} section={s} active={activeList === s.list} />
      ))}
      <div className="nav-sep" />
      <button className={`nav-item${route.screen === "snapshots" ? " active" : ""}`} onClick={() => navigate({ screen: "snapshots" })}>
        <span className="nav-row">
          <CalendarClock size="1.077rem" />
          <span className="nav-label">Snapshots & diff</span>
        </span>
      </button>
    </nav>
  );
}

function Header() {
  const { stack, index } = useStore((s) => s.wpNav);
  const route = useStore(selectWpRoute);
  const meta = useStore((s) => s.entityMeta);
  const { goBack, goForward, setPaletteOpen, navigate } = useStore.getState();
  const metaKey = route.screen === "entity" ? `${route.entity}:${route.id}` : null;
  const crumbs = routeCrumbs(route, meta && meta.key === metaKey ? meta : null);
  return (
    <div className="wp-header">
      <button className="icon-btn" aria-label="Back" title="Back (⌘[)" disabled={index === 0} onClick={goBack}>
        <ArrowLeft size="1.077rem" />
      </button>
      <button className="icon-btn" aria-label="Forward" title="Forward (⌘])" disabled={index >= stack.length - 1} onClick={goForward}>
        <ArrowRight size="1.077rem" />
      </button>
      <nav className="crumbs" aria-label="Breadcrumbs">
        {crumbs.map((c, i) => {
          const current = i === crumbs.length - 1;
          return (
            <span key={`${i}:${c.label}`} className={current ? "crumb current" : "crumb"}>
              {i > 0 && <ChevronRight size="0.846rem" />}
              {c.route && !current ? <button onClick={() => navigate(c.route!)}>{c.label}</button> : <span aria-current={current ? "page" : undefined}>{c.label}</span>}
            </span>
          );
        })}
      </nav>
      <div className="spacer" />
      <button className="search-trigger" onClick={() => setPaletteOpen(true)}>
        <Search size="1rem" /> Search or run a command… <kbd>⌘K</kbd>
      </button>
    </div>
  );
}

export function WpWorkspace() {
  const route = useStore(selectWpRoute);
  const wp = useStore(selectWp);
  if (!wp) return <ErrorPanel message="This database is not a WordPress install." code={null} />;
  return (
    <div className="wp-workspace">
      <Nav route={route} />
      <main className="wp-main">
        <Header />
        <div className="wp-content">
          {route.screen === "dashboard" && <DashboardScreen />}
          {route.screen === "list" && (route.list === "settings" ? <SettingsScreen key={JSON.stringify(route.params ?? {})} params={route.params} /> : <ListScreen key={route.list} list={route.list} params={route.params} />)}
          {route.screen === "entity" && <EntityScreen key={`${route.entity}:${route.id}`} entity={route.entity} id={route.id} />}
          {route.screen === "compare" && <CompareScreen entity={route.entity} a={route.a} b={route.b} />}
          {route.screen === "snapshots" && <SnapshotsScreen />}
        </div>
      </main>
    </div>
  );
}
