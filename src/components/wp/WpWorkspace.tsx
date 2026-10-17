import { ArrowLeft, ArrowRight, Camera, ChevronRight, LayoutDashboard, Search } from "lucide-react";
import { routeCrumbs } from "../../lib/crumbs";
import { selectWp, selectWpRoute, useStore, type WpRoute } from "../../state/store";
import { ErrorPanel } from "../ErrorPanel";
import { DashboardScreen } from "./Dashboard";
import { SnapshotsScreen } from "./Debugging";

function Nav({ route }: { route: WpRoute }) {
  const navigate = useStore((s) => s.navigate);
  return (
    <nav className="wp-nav" aria-label="WordPress" data-stage="wp:nav" data-stage-label="WordPress navigation">
      <button data-stage="wp:nav:dashboard" data-stage-list="wp-nav" aria-current={route.screen === "dashboard" ? "page" : undefined} className={`nav-item${route.screen === "dashboard" ? " active" : ""}`} onClick={() => navigate({ screen: "dashboard" })}>
        <span className="nav-row">
          <LayoutDashboard size="1.077rem" />
          <span className="nav-label">Dashboard</span>
        </span>
      </button>
      <div className="nav-group" role="group" aria-label="Tools">
        <div className="nav-heading">Tools</div>
        <button data-stage="wp:nav:snapshots" data-stage-list="wp-nav" aria-current={route.screen === "snapshots" ? "page" : undefined} className={`nav-item${route.screen === "snapshots" ? " active" : ""}`} onClick={() => navigate({ screen: "snapshots" })}>
          <span className="nav-row">
            <Camera size="1.077rem" />
            <span className="nav-label">Snapshots & diff</span>
          </span>
        </button>
      </div>
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
  if (!wp) return <ErrorPanel message="This database is not a WordPress install." code={null} />;
  return (
    <div className="wp-workspace">
      <Nav route={route} />
      <main className="wp-main">
        <Header />
        <div className="wp-content" data-stage="wp:content" data-stage-label={`WordPress ${route.screen}`}>
          {route.screen === "dashboard" && <DashboardScreen />}
          {route.screen === "snapshots" && <SnapshotsScreen />}
        </div>
      </main>
    </div>
  );
}
