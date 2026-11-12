/** Shared controls for settings pages: grouped cards and rows that apply their change right away. */
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";

export const capitalize = (s: string) => s[0].toUpperCase() + s.slice(1);

/** `anchor`: the row's id in its page's `rows`, found by the Settings search. */
type Anchored = { anchor?: string };
/** `stage`: a Stage anchor (`data-stage`) a presentation can point at. */
type Staged = Anchored & { stage?: string };

/** The row Settings was opened at; a new object per request, so opening the same row again reveals it again. */
export interface RevealTarget {
  id: string;
  /** Revealed instead while the row isn't shown. */
  under?: string;
}

export const ScrollTarget = createContext<RevealTarget | undefined>(undefined);

/** Targets whose own row was revealed; a fallback revealed first gives way to the row when it mounts later. */
const revealed = new WeakSet<RevealTarget>();

function scrollAndHighlight(element: HTMLElement) {
  element.scrollIntoView?.({ block: "start" });
  element.classList.remove("found");
  void element.offsetWidth; // restarts the highlight
  element.classList.add("found");
}

function useScrollTarget<E extends HTMLElement>(anchor: string | undefined) {
  const target = useContext(ScrollTarget);
  const ref = useRef<E>(null);
  useEffect(() => {
    const element = ref.current;
    if (!anchor || !target || !element || revealed.has(target)) return;
    if (anchor !== target.id && anchor !== target.under) return;
    if (anchor === target.id) revealed.add(target);
    else if (element.ownerDocument.querySelector(`[data-anchor="${CSS.escape(target.id)}"]`)) return;
    scrollAndHighlight(element);
  }, [anchor, target]);
  return ref;
}

/** `list`: the rows come from data (connections, saved slugs), so they have no anchors of their own; the search finds the group. */
type GroupProps = Anchored & { title?: string; list?: boolean; children: ReactNode };

export function Group({ title, anchor, list, children }: GroupProps) {
  const ref = useScrollTarget<HTMLElement>(anchor);
  return (
    <section ref={ref} className="settings-group" aria-label={title} data-anchor={anchor} data-list={list || undefined}>
      {title && <h3>{title}</h3>}
      <div className="settings-card">{children}</div>
    </section>
  );
}

/** One card row: label and description on the left, the control on the right (or below when `stacked`). */
type RowProps = Anchored & { label: string; description?: ReactNode; htmlFor?: string; stacked?: boolean; children: ReactNode };

export function Row({ label, description, htmlFor, stacked, anchor, children }: RowProps) {
  const ref = useScrollTarget<HTMLDivElement>(anchor);
  return (
    <div ref={ref} className={`settings-row${stacked ? " stacked" : ""}`} data-anchor={anchor}>
      <div className="settings-row-text">
        {htmlFor ? <label htmlFor={htmlFor}>{label}</label> : <div className="settings-row-label">{label}</div>}
        {description && <div className="settings-row-desc">{description}</div>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

type SwitchProps = Anchored & { label: string; description?: ReactNode; checked: boolean; onChange: (v: boolean) => void };

export function Switch({ label, description, anchor, checked, onChange }: SwitchProps) {
  const id = useId();
  return (
    <Row label={label} description={description} htmlFor={id} anchor={anchor}>
      <input id={id} className="switch" type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </Row>
  );
}

/** The bare segmented buttons, for rows that show more than the control (a preview). */
type SegmentedButtonsProps<T extends string> = Staged & {
  label: string;
  options: readonly T[];
  value: T;
  onChange: (v: T) => void;
};

export function SegmentedButtons<T extends string>({ label, options, value, onChange, stage }: SegmentedButtonsProps<T>) {
  return (
    <div className="segmented" role="group" aria-label={label} data-stage={stage} data-stage-label={stage && label}>
      {options.map((o) => (
        <button key={o} type="button" aria-pressed={value === o} data-stage={stage && `${stage}:${o}`} data-stage-list={stage} onClick={() => onChange(o)}>
          {capitalize(o)}
        </button>
      ))}
    </div>
  );
}

export function Segmented<T extends string>({ label, description, anchor, ...buttons }: SegmentedButtonsProps<T> & { description?: ReactNode }) {
  return (
    <Row label={label} description={description} anchor={anchor}>
      <SegmentedButtons label={label} {...buttons} />
    </Row>
  );
}

/** Text input that commits on blur or Enter, so half-typed values never reach the backend.
 *  Number spinner steps (input without typing) also commit, debounced, so they preview right away. */
export function TextField({ label, description, value, onCommit, type = "text", min, max, suffix, placeholder, stage, anchor }: Staged & {
  label: string;
  description?: ReactNode;
  value: string;
  onCommit: (v: string) => void;
  type?: "text" | "number";
  min?: number;
  max?: number;
  suffix?: string;
  placeholder?: string;
}) {
  const id = useId();
  const [text, setText] = useState(value);
  const stepTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => setText(value), [value]);
  useEffect(() => () => clearTimeout(stepTimer.current), []);

  const inRange = (draft: string) => {
    if (type !== "number") return true;
    const n = Number(draft);
    return draft.trim() !== "" && n >= (min ?? -Infinity) && n <= (max ?? Infinity);
  };
  const commit = (draft: string) => {
    clearTimeout(stepTimer.current);
    if (draft === value) return;
    if (inRange(draft)) onCommit(draft);
    else setText(value);
  };

  return (
    <Row label={label} description={description} htmlFor={id} anchor={anchor}>
      <div className="settings-input">
        <input
          id={id}
          className={`input${type === "number" ? " number" : ""}`}
          type={type}
          min={min}
          max={max}
          placeholder={placeholder}
          data-stage={stage}
          data-stage-label={stage && label}
          value={text}
          onChange={(e) => {
            const next = e.target.value;
            setText(next);
            // Typing reports an inputType; spinner clicks and arrow-key steps do not.
            const typed = !!(e.nativeEvent as InputEvent).inputType;
            if (type !== "number" || typed || !inRange(next)) return;
            clearTimeout(stepTimer.current);
            stepTimer.current = setTimeout(() => commit(next), 300);
          }}
          onBlur={() => commit(text)}
          onKeyDown={(e) => e.key === "Enter" && commit(text)}
        />
        {suffix && <span className="muted">{suffix}</span>}
      </div>
    </Row>
  );
}

export function NumberField({ value, onCommit, ...rest }: Staged & {
  label: string;
  description?: ReactNode;
  value: number;
  onCommit: (v: number) => void;
  min: number;
  max: number;
  suffix?: string;
}) {
  return (
    <TextField
      {...rest}
      type="number"
      value={String(value)}
      onCommit={(t) => {
        const n = Number(t);
        if (t.trim() !== "" && Number.isFinite(n)) onCommit(n);
      }}
    />
  );
}
