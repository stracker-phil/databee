import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { isRecordingTarget } from "../commands/registry";

interface Props {
  onClose: () => void;
  /** `sheet`: the padded dialog; `panel`: the overlay surface, laid out by its content; `bare`: neither. */
  variant?: "sheet" | "panel" | "bare";
  className?: string;
  labelledBy?: string;
  label?: string;
  /** Exposes the sheet as its own window to Stage (src/stage/anchors.ts). */
  stageWindow?: string;
  /** An accent for the sheet instead of the one in effect (the open database's). */
  accent?: string;
  children: ReactNode;
}

const FOCUSABLE = 'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hidden);
}

/** Keeps Tab inside the sheet and returns focus to the element that opened it. */
export function Modal({ onClose, variant = "sheet", className = "", labelledBy, label, stageWindow, accent, children }: Props) {
  const sheetRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  // Read during the first render: children with autoFocus take focus before any effect runs.
  const [opener] = useState(() => document.activeElement as HTMLElement | null);

  useEffect(() => {
    const sheet = sheetRef.current;
    if (sheet && !sheet.contains(document.activeElement)) (focusables(sheet)[0] ?? sheet).focus();
    return () => {
      // An action run on close may have focused its own target (a new tab, the WHERE field); keep that.
      const active = document.activeElement;
      const focusLost = !active || active === document.body || !!sheet?.contains(active);
      if (focusLost && opener?.isConnected) opener.focus();
    };
  }, [opener]);

  useEffect(() => {
    // Capture phase, stopped here: controls inside the sheet and bubbling listeners never also act on Escape.
    // The app's shortcut handler already ignores keys while an overlay is open, except settings and zoom.
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (isRecordingTarget(e.target)) return;
      e.stopPropagation();
      onCloseRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const trapTab = (e: KeyboardEvent) => {
    if (e.key !== "Tab" || !sheetRef.current) return;
    const items = focusables(sheetRef.current);
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === sheetRef.current)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return createPortal(
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={sheetRef}
        className={["dialog", variant === "bare" ? "" : variant, className].filter(Boolean).join(" ")}
        data-stage={stageWindow ? `${stageWindow}:sheet` : "dialog"}
        data-stage-window={stageWindow}
        data-accent={accent}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-label={label}
        tabIndex={-1}
        onKeyDown={trapTab}
      >
        {children}
      </div>
    </div>,
    document.body,
  );
}
