/** Table icons (`[[icon]]` rules): the curated icon names mapped to their lucide components, and the icon of a table. */
import { useEffect, type ReactNode } from "react";
import {
  Activity,
  Archive,
  Bell,
  Bookmark,
  Box,
  Calendar,
  ChartBar,
  Clock,
  Code,
  Coins,
  CreditCard,
  Database,
  FileText,
  Files,
  Folder,
  Globe,
  Hash,
  Image,
  Key,
  Layers,
  Link,
  List,
  ListChecks,
  Lock,
  Mail,
  MapPin,
  Menu,
  MessageSquare,
  Package,
  Receipt,
  Settings,
  Shield,
  ShoppingCart,
  Star,
  Store,
  Tag,
  Tags,
  Ticket,
  Truck,
  User,
  Users,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { TableIcon as IconName } from "../api";
import { projectKey } from "../state/slices/project";
import { useStore } from "../state/store";

/** Every icon a rule may name except `none`; typed by the Rust enum, so both sides list the same names. */
export const TABLE_ICONS: Record<Exclude<IconName, "none">, LucideIcon> = {
  user: User,
  users: Users,
  menu: Menu,
  "file-text": FileText,
  files: Files,
  "message-square": MessageSquare,
  tag: Tag,
  tags: Tags,
  folder: Folder,
  settings: Settings,
  "shopping-cart": ShoppingCart,
  receipt: Receipt,
  package: Package,
  "credit-card": CreditCard,
  wallet: Wallet,
  truck: Truck,
  clock: Clock,
  calendar: Calendar,
  list: List,
  "list-checks": ListChecks,
  database: Database,
  key: Key,
  lock: Lock,
  link: Link,
  image: Image,
  mail: Mail,
  globe: Globe,
  "map-pin": MapPin,
  star: Star,
  bell: Bell,
  box: Box,
  archive: Archive,
  layers: Layers,
  hash: Hash,
  activity: Activity,
  "chart-bar": ChartBar,
  coins: Coins,
  store: Store,
  ticket: Ticket,
  bookmark: Bookmark,
  shield: Shield,
  code: Code,
};

export const ICON_NAMES = Object.keys(TABLE_ICONS) as Exclude<IconName, "none">[];

/**
 * The icon of real table `table` in `database` of session `sessionId` (default: the active session), from the
 * database's `rulesInfo`, which it loads when missing; `null` without one.
 */
export function useTableIcon(database: string | null | undefined, table: string | null | undefined, sessionId?: string): IconName | null {
  const session = useStore((s) => sessionId ?? s.activeSessionId);
  const key = session && database ? projectKey(session, database) : null;
  const loaded = useStore((s) => !key || key in s.rulesInfo);
  const icon = useStore((s) => (key && table ? (s.rulesInfo[key]?.tableIcons[table] ?? null) : null));
  useEffect(() => {
    if (!loaded && session && database) useStore.getState().ensureRulesInfo(session, database);
  }, [loaded, session, database]);
  return icon;
}

/** One icon, sized to the text around it; decorative, the table name beside it says the same. */
export function IconGlyph({ name, className = "tbl-icon" }: { name: IconName | null; className?: string }) {
  if (!name || name === "none") return null;
  const Icon = TABLE_ICONS[name];
  return <Icon className={className} size="1em" aria-hidden="true" data-icon={name} />;
}

interface Props {
  database: string | null | undefined;
  table: string | null | undefined;
  sessionId?: string;
  /** Shown when the table has no icon. */
  fallback?: ReactNode;
}

/** The icon of a table, before its name; nothing (or `fallback`) when it has none. */
export function TableIcon({ database, table, sessionId, fallback = null }: Props) {
  const name = useTableIcon(database, table, sessionId);
  return name ? <IconGlyph name={name} /> : <>{fallback}</>;
}
