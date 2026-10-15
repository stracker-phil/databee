import type { ReactNode } from "react";
import { initials, relativeFromUnix, absoluteFromUnix } from "../../lib/wpFormat";

const STATUS_TONES: Record<string, string> = {
  completed: "green",
  complete: "green",
  active: "green",
  publish: "green",
  instock: "green",
  processing: "blue",
  "in-progress": "blue",
  pending: "gray",
  draft: "gray",
  "checkout-draft": "gray",
  "on-hold": "orange",
  onbackorder: "orange",
  failed: "red",
  cancelled: "red",
  canceled: "red",
  outofstock: "red",
  expired: "red",
  refunded: "purple",
  "pending-cancel": "orange",
  private: "purple",
};

export function statusTone(status: string): string {
  return STATUS_TONES[status.replace(/^wc-/, "")] ?? "gray";
}

export function statusText(status: string): string {
  const s = status.replace(/^wc-/, "").replace(/[-_]/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function StatusPill({ status, label }: { status: string; label?: string }) {
  return <span className={`pill tone-${statusTone(status)}`}>{label ?? statusText(status)}</span>;
}

export function Ago({ unix }: { unix: number | null | undefined }) {
  if (unix == null) return <span className="muted">—</span>;
  return <span title={absoluteFromUnix(unix)}>{relativeFromUnix(unix)}</span>;
}

export function Avatar({ name, src, size = "2.154rem" }: { name: string; src?: string | null; size?: string }) {
  const hue = [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  return (
    <span className="avatar pii" style={{ width: size, height: size, fontSize: `calc(${size} * 0.38)`, background: `hsl(${hue} 45% 45%)` }}>
      {src ? <img src={src} alt="" onError={(e) => (e.currentTarget.style.display = "none")} /> : null}
      <span>{initials(name)}</span>
    </span>
  );
}

export function Stat({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: string }) {
  return (
    <div className="stat-tile">
      <div className="stat-label">{label}</div>
      <div className={`stat-value${tone ? ` tone-text-${tone}` : ""}`}>{value}</div>
      {sub && <div className={`stat-sub${tone ? ` tone-text-${tone}` : ""}`}>{sub}</div>}
    </div>
  );
}

export function Card({ title, aside, children, className = "" }: { title?: ReactNode; aside?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`card ${className}`}>
      {(title || aside) && (
        <header className="card-head">
          <h3>{title}</h3>
          <span className="muted">{aside}</span>
        </header>
      )}
      {children}
    </section>
  );
}

export function Thumb({ src, size = "2.769rem" }: { src: string | null | undefined; size?: string }) {
  return src ? (
    <img className="thumb" src={src} alt="" loading="lazy" style={{ width: size, height: size }} />
  ) : (
    <span className="thumb thumb-empty" style={{ width: size, height: size }} />
  );
}

export function Loading() {
  return <div className="empty-state">Loading…</div>;
}
