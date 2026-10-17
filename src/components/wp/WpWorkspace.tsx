import { ArrowLeft, ArrowRight, Boxes, CalendarClock, Camera, ChevronRight, FileText, LayoutDashboard, Repeat, Search, Settings2, ShoppingBag, ShoppingCart, Users } from "lucide-react";
import { api, type BrowserSection } from "../../api";
import { routeCrumbs } from "../../lib/crumbs";
import { formatCount } from "../../lib/format";
import { selectWp, selectWpRoute, useStore, type WpList, type WpRoute } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DashboardScreen } from "./Dashboard";
import { ListScreen } from "./Lists";
import { SettingsScreen } from "./Settings";
import { SnapshotsScreen } from "./Debugging";
import { FindScreen } from "./FindScreen";
import { useWpData } from "./useWpData";

const ICONS: Record<string, typeof ShoppingCart> = {
  orders: ShoppingCart,
  subscriptions: Repeat,
  users: Users,
  products: ShoppingBag,
  actions: CalendarClock,
  sessions: Boxes,
  content: FileText,
  settings: Settings2,
};

/** Sidebar groups in wp-admin's order: core data, then WooCommerce, then tools (wp-admin
 *  files Scheduled Actions under Tools). Sections the database doesn't have are skipped. */
const NAV_GROUPS: { title: string; lists: WpList[] }[] = [
  { title: "WordPress", lists: ["content", "users", "settings"] },
  { title: "WooCommerce", lists: ["orders", "subscriptions", "products", "sessions"] },
  { title: "Tools", lists: ["actions"] },
];

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
    case "snapshots":
      return "Snapshots";
    case "find":
      return `Find “${route.value}”`;
  }
}

function NavItem({ section, active }: { section: BrowserSection; active: boolean }) {
  const navigate = useStore((s) => s.navigate);
  const Icon = ICONS[section.list] ?? FileText;
  // Only the most severe badge fits the row; the rest are in the tooltip.
  const badges = attentionBadges(section);
  return (
    <button data-stage={`wp:nav:${section.list}`} data-stage-list="wp-nav" aria-current={active ? "page" : undefined} className={`nav-item${active ? " active" : ""}`} onClick={() => navigate({ screen: "list", list: section.list })}>
      <span className="nav-row">
        <Icon size="1.077rem" />
        <span className="nav-label" data-stage-label={section.label}>{section.label}</span>
        {badges.length > 0 && (
          <span className={`nav-badge tone-${badges[0].tone}`} title={badges.map((b) => `${formatCount(b.count)} ${b.label}`).join(" · ")}>
            {formatCount(badges[0].count)} {badges[0].label}
            {badges.length > 1 && ` +${badges.length - 1}`}
          </span>
        )}
        <span className="nav-count">{formatCount(section.count)}</span>
      </span>
    </button>
  );
}

function Nav({ route }: { route: WpRoute }) {
  const navigate = useStore((s) => s.navigate);
  const { data } = useWpData([], api.wpBrowser);
  const activeList = route.screen === "list" ? route.list : null;
  return (
    <nav className="wp-nav" aria-label="WordPress" data-stage="wp:nav" data-stage-label="WordPress navigation">
      <button data-stage="wp:nav:dashboard" data-stage-list="wp-nav" aria-current={route.screen === "dashboard" ? "page" : undefined} className={`nav-item${route.screen === "dashboard" ? " active" : ""}`} onClick={() => navigate({ screen: "dashboard" })}>
        <span className="nav-row">
          <LayoutDashboard size="1.077rem" />
          <span className="nav-label">Dashboard</span>
        </span>
      </button>
      {NAV_GROUPS.map((g) => {
        const sections = g.lists.map((l) => data?.find((s) => s.list === l)).filter((s) => s != null);
        if (sections.length === 0 && g.title !== "Tools") return null;
        return (
          <div key={g.title} className="nav-group" role="group" aria-label={g.title}>
            <div className="nav-heading">{g.title}</div>
            {sections.map((s) => (
              <NavItem key={s.list} section={s} active={activeList === s.list} />
            ))}
            {g.title === "Tools" && (
              <button data-stage="wp:nav:snapshots" data-stage-list="wp-nav" aria-current={route.screen === "snapshots" ? "page" : undefined} className={`nav-item${route.screen === "snapshots" ? " active" : ""}`} onClick={() => navigate({ screen: "snapshots" })}>
                <span className="nav-row">
                  <Camera size="1.077rem" />
                  <span className="nav-label">Snapshots & diff</span>
                </span>
              </button>
            )}
          </div>
        );
      })}
    </nav>
  );
}

function Header() {
  const { stack, index } = useStore((s) => s.wpNav);
  const route = useStore(selectWpRoute);
  const { goBack, goForward, setPaletteOpen, navigate } = useStore.getState();
  const crumbs = routeCrumbs(route);
  return (
    <div className="wp-header" data-stage="wp:header" data-stage-label="Header">
      <button data-stage="wp:back" className="icon-btn" aria-label="Back" title="Back (⌘[)" disabled={index === 0} onClick={goBack}>
        <ArrowLeft size="1.077rem" />
      </button>
      <button data-stage="wp:forward" className="icon-btn" aria-label="Forward" title="Forward (⌘])" disabled={index >= stack.length - 1} onClick={goForward}>
        <ArrowRight size="1.077rem" />
      </button>
      <nav className="crumbs" aria-label="Breadcrumbs" data-stage="wp:crumbs">
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
      <button data-stage="wp:search" data-stage-label="Search or run a command" className="search-trigger" onClick={() => setPaletteOpen(true)}>
        <Search size="1rem" /> Search or run a command… <kbd>⌘K</kbd>
      </button>
    </div>
  );
}

export function WpWorkspace() {
  const route = useStore(selectWpRoute);
  const wp = useStore(selectWp);
  // A list remounts per history entry, so back and forward also restore its search input.
  const historyIndex = useStore((s) => s.wpNav.index);
  if (!wp) return <ErrorPanel message="This database is not a WordPress install." code={null} />;
  return (
    <div className="wp-workspace">
      <Nav route={route} />
      <main className="wp-main">
        <Header />
        <div className="wp-content" data-stage="wp:content" data-stage-label={`WordPress ${route.screen}`}>
          {route.screen === "dashboard" && <DashboardScreen />}
          {/* ListScreen seeds its search input once, so back and forward must remount it via the history index. */}
          {route.screen === "list" && (route.list === "settings" ? <SettingsScreen key={JSON.stringify(route.params ?? {})} params={route.params} /> : <ListScreen key={`${route.list}:${historyIndex}`} list={route.list} params={route.params} />)}
          {route.screen === "snapshots" && <SnapshotsScreen />}
          {route.screen === "find" && <FindScreen key={route.value} value={route.value} />}
        </div>
      </main>
    </div>
  );
}
