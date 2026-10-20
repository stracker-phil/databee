/** Table icons (`[[icon]]` rules): the curated icon names mapped to their lucide components, and the icon of a table. */
import { useEffect, type ReactNode } from "react";
import {
  Activity,
  Archive,
  BadgePercent,
  Banknote,
  Barcode,
  Bell,
  Blocks,
  BookOpen,
  Bookmark,
  Box,
  Briefcase,
  Bug,
  Building,
  Calendar,
  CalendarClock,
  ChartBar,
  Clock,
  Code,
  Coins,
  CreditCard,
  Database,
  FileIcon,
  FileBraces,
  FileClock,
  FileCode,
  FileText,
  Files,
  Film,
  Flag,
  Folder,
  FolderTree,
  Gift,
  GitBranch,
  Globe,
  GraduationCap,
  Hash,
  Heart,
  IdCard,
  Image,
  Inbox,
  Key,
  KeyRound,
  Landmark,
  Languages,
  Layers,
  Link,
  List,
  ListChecks,
  Lock,
  LogIn,
  Mail,
  MapIcon,
  MapPin,
  Megaphone,
  Menu,
  MessageCircle,
  MessageSquare,
  Music,
  Newspaper,
  Package,
  Paperclip,
  Phone,
  Plug,
  Puzzle,
  Receipt,
  Route,
  Send,
  Server,
  Settings,
  Shield,
  ShieldCheck,
  ShoppingBag,
  ShoppingCart,
  SlidersHorizontal,
  Star,
  Store,
  Table2,
  Tag,
  Tags,
  Terminal,
  Ticket,
  Timer,
  Truck,
  User,
  UserCog,
  UserRound,
  Users,
  Wallet,
  Warehouse,
  Webhook,
  Workflow,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { TableIcon as IconName } from "../api";
import { projectKey } from "../state/slices/project";
import { useStore } from "../state/store";
import { OpenSwap } from "./value/OpenHint";
import type { ClickEffect } from "./value/open";

/** Every icon a rule may name; typed by the Rust enum, so both sides list the same names. */
export const TABLE_ICONS: Record<IconName, LucideIcon> = {
  table: Table2,
  database: Database,
  server: Server,
  list: List,
  "list-checks": ListChecks,
  layers: Layers,
  box: Box,
  archive: Archive,
  hash: Hash,
  key: Key,
  "key-round": KeyRound,
  lock: Lock,
  "log-in": LogIn,
  shield: Shield,
  "shield-check": ShieldCheck,
  user: User,
  "user-round": UserRound,
  users: Users,
  "user-cog": UserCog,
  "id-card": IdCard,
  "file-text": FileText,
  files: Files,
  file: FileIcon,
  "file-code": FileCode,
  "file-braces": FileBraces,
  "file-clock": FileClock,
  newspaper: Newspaper,
  "book-open": BookOpen,
  "graduation-cap": GraduationCap,
  image: Image,
  film: Film,
  music: Music,
  paperclip: Paperclip,
  folder: Folder,
  "folder-tree": FolderTree,
  menu: Menu,
  tag: Tag,
  tags: Tags,
  bookmark: Bookmark,
  star: Star,
  heart: Heart,
  flag: Flag,
  "message-square": MessageSquare,
  "message-circle": MessageCircle,
  mail: Mail,
  inbox: Inbox,
  send: Send,
  bell: Bell,
  megaphone: Megaphone,
  phone: Phone,
  "shopping-cart": ShoppingCart,
  "shopping-bag": ShoppingBag,
  store: Store,
  receipt: Receipt,
  package: Package,
  barcode: Barcode,
  truck: Truck,
  warehouse: Warehouse,
  "credit-card": CreditCard,
  wallet: Wallet,
  banknote: Banknote,
  coins: Coins,
  landmark: Landmark,
  "badge-percent": BadgePercent,
  gift: Gift,
  ticket: Ticket,
  building: Building,
  briefcase: Briefcase,
  globe: Globe,
  "map-pin": MapPin,
  map: MapIcon,
  route: Route,
  languages: Languages,
  clock: Clock,
  calendar: Calendar,
  "calendar-clock": CalendarClock,
  timer: Timer,
  settings: Settings,
  "sliders-horizontal": SlidersHorizontal,
  wrench: Wrench,
  activity: Activity,
  "chart-bar": ChartBar,
  code: Code,
  terminal: Terminal,
  bug: Bug,
  webhook: Webhook,
  plug: Plug,
  puzzle: Puzzle,
  blocks: Blocks,
  "git-branch": GitBranch,
  workflow: Workflow,
  link: Link,
};

export const ICON_NAMES = Object.keys(TABLE_ICONS) as IconName[];

/** The icon of a table no rule gives one, and of every table until its database's rules are loaded. */
export const DEFAULT_ICON: IconName = "table";

/**
 * The icon of real table `table` in `database` of session `sessionId` (default: the active session), from the
 * database's `rulesInfo`, which it loads when missing; `DEFAULT_ICON` until then; `null` without a table.
 */
export function useTableIcon(database: string | null | undefined, table: string | null | undefined, sessionId?: string): IconName | null {
  const session = useStore((s) => sessionId ?? s.activeSessionId);
  const key = session && database ? projectKey(session, database) : null;
  const loaded = useStore((s) => !key || key in s.rulesInfo);
  const icon = useStore((s) => (key && table ? (s.rulesInfo[key]?.tableIcons[table] ?? null) : null));
  useEffect(() => {
    if (!loaded && session && database) useStore.getState().ensureRulesInfo(session, database);
  }, [loaded, session, database]);
  return table ? (icon ?? DEFAULT_ICON) : null;
}

/** One icon, sized to the text around it; decorative, the table name beside it says the same. */
export function IconGlyph({ name, className = "tbl-icon" }: { name: IconName | null; className?: string }) {
  if (!name) return null;
  const Icon = TABLE_ICONS[name];
  return <Icon className={className} size="1em" aria-hidden="true" data-icon={name} />;
}

interface Props {
  database: string | null | undefined;
  table: string | null | undefined;
  sessionId?: string;
  /** What a click on the link the icon leads does: the icon turns into its indicator while hovered (`OpenSwap`). */
  effect?: ClickEffect | null;
  /** Shown without a table. */
  fallback?: ReactNode;
}

/** The icon of a table, before its name or a link into it; nothing (or `fallback`) without a table. */
export function TableIcon({ database, table, sessionId, effect = null, fallback = null }: Props) {
  const name = useTableIcon(database, table, sessionId);
  if (!name) return <>{fallback}</>;
  return (
    <OpenSwap effect={effect}>
      <IconGlyph name={name} />
    </OpenSwap>
  );
}
