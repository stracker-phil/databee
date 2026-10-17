import type { ReactNode } from "react";

export function Stat({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: ReactNode; tone?: string }) {
  return (
    <div className="stat-tile" role="group" aria-label={label}>
      <div className="stat-label">{label}</div>
      <div className={`stat-value${tone ? ` tone-text-${tone}` : ""}`}>{value}</div>
      {sub && <div className={`stat-sub${tone ? ` tone-text-${tone}` : ""}`}>{sub}</div>}
    </div>
  );
}

export function Card({ title, aside, children, className = "" }: { title?: ReactNode; aside?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`card ${className}`} aria-label={typeof title === "string" ? title : undefined}>
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

export function Loading() {
  return <div className="empty-state">Loading…</div>;
}
