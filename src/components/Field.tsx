import type { ReactNode } from "react";

/** A key combination as a small cap (`⌘K`); renders nothing for an unbound command. */
export function KeyCap({ keys }: { keys: string }) {
  if (!keys) return null;
  return <kbd className="keycap">{keys}</kbd>;
}

interface InsetFieldProps {
  icon: ReactNode;
  /** Key cap text, e.g. `⌘⇧F`; empty for none. */
  keys?: string;
  className?: string;
  children: ReactNode;
}

/**
 * The macOS field frame (docs/ux.md, Inputs): a leading icon, the control, an optional key cap. The frame is
 * a label, so a click anywhere on it focuses the input.
 */
export function InsetField({ icon, keys = "", className = "", children }: InsetFieldProps) {
  return (
    <label className={`inset-field ${className}`.trim()}>
      <span className="inset-field-icon" aria-hidden="true">{icon}</span>
      {children}
      <KeyCap keys={keys} />
    </label>
  );
}
