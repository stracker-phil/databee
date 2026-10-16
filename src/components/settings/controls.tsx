/** Shared controls for settings pages: grouped cards and rows that apply their change right away. */
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

export const capitalize = (s: string) => s[0].toUpperCase() + s.slice(1);

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="settings-group">
      {title && <h3>{title}</h3>}
      <div className="settings-card">{children}</div>
    </section>
  );
}

/** One card row: label and description on the left, the control on the right (or below when `stacked`). */
export function Row({ label, description, htmlFor, stacked, children }: { label: string; description?: ReactNode; htmlFor?: string; stacked?: boolean; children: ReactNode }) {
  return (
    <div className={`settings-row${stacked ? " stacked" : ""}`}>
      <div className="settings-row-text">
        {htmlFor ? <label htmlFor={htmlFor}>{label}</label> : <div className="settings-row-label">{label}</div>}
        {description && <div className="settings-row-desc">{description}</div>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

export function Switch({ label, description, checked, onChange }: { label: string; description?: ReactNode; checked: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <Row label={label} description={description} htmlFor={id}>
      <input id={id} className="switch" type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </Row>
  );
}

export function Segmented<T extends string>({ label, description, options, value, onChange }: { label: string; description?: ReactNode; options: readonly T[]; value: T; onChange: (v: T) => void }) {
  return (
    <Row label={label} description={description}>
      <div className="segmented" role="group" aria-label={label}>
        {options.map((o) => (
          <button key={o} type="button" aria-pressed={value === o} onClick={() => onChange(o)}>
            {capitalize(o)}
          </button>
        ))}
      </div>
    </Row>
  );
}

/** Text input that commits on blur or Enter, so half-typed values never reach the backend.
 *  Number spinner steps (input without typing) also commit, debounced, so they preview right away. */
export function TextField({ label, description, value, onCommit, type = "text", min, max, suffix }: {
  label: string;
  description?: ReactNode;
  value: string;
  onCommit: (v: string) => void;
  type?: "text" | "number";
  min?: number;
  max?: number;
  suffix?: string;
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
    <Row label={label} description={description} htmlFor={id}>
      <div className="settings-input">
        <input
          id={id}
          className={`input${type === "number" ? " number" : ""}`}
          type={type}
          min={min}
          max={max}
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

export function NumberField({ value, onCommit, ...rest }: { label: string; description?: ReactNode; value: number; onCommit: (v: number) => void; min: number; max: number; suffix?: string }) {
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
