import { ChevronsUpDown } from "lucide-react";
import { useState } from "react";
import { MenuPanel, type PanelEntry } from "./MenuPanel";

export interface PopUpOption<T extends string> {
  value: T;
  label: string;
}

/** Options under a caption, e.g. the "Light" and "Dark" schemes. */
export interface PopUpGroup<T extends string> {
  caption: string;
  options: readonly PopUpOption<T>[];
}

interface Props<T extends string> {
  value: T;
  options: readonly (PopUpOption<T> | PopUpGroup<T>)[];
  onChange: (value: T) => void;
  "aria-label"?: string;
  /** For a `<label htmlFor>` naming it. */
  id?: string;
  className?: string;
  /** The button and its menu in the data font, for table and column names. */
  mono?: boolean;
  /** Reads as plain text (the toolbar): no chevron, as wide as the current label. */
  plain?: boolean;
  disabled?: boolean;
  "data-stage"?: string;
  "data-stage-label"?: string;
}

const isGroup = <T extends string>(o: PopUpOption<T> | PopUpGroup<T>): o is PopUpGroup<T> => "caption" in o;

/** The app's replacement for a native `<select>`: the current choice and a chevron, as wide as its widest choice,
 *  opening the choices in the shared menu panel with a check on the current one. */
export function PopUpButton<T extends string>(props: Props<T>) {
  const { value, options, onChange, id, className, mono, plain, disabled } = props;
  const [button, setButton] = useState<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const flat = options.flatMap((o) => (isGroup(o) ? o.options : [o]));
  const current = flat.find((o) => o.value === value);

  const entry = (o: PopUpOption<T>): PanelEntry => ({ label: o.label, checked: o.value === value, onChoose: () => o.value !== value && onChange(o.value) });
  const entries = options.flatMap((o): PanelEntry[] => (isGroup(o) ? [{ caption: o.caption }, ...o.options.map(entry)] : [entry(o)]));
  const classes = ["popup-button", plain ? "plain" : "", mono ? "mono" : "", className ?? ""].filter(Boolean).join(" ");

  return (
    <>
      <button
        ref={setButton}
        type="button"
        id={id}
        className={classes}
        aria-label={props["aria-label"]}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        data-stage={props["data-stage"]}
        data-stage-label={props["data-stage-label"]}
        onClick={() => setOpen(!open)}
        onKeyDown={(e) => {
          if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
          e.preventDefault();
          setOpen(true);
        }}
      >
        <span className="popup-button-value">
          <span>{current?.label ?? ""}</span>
          {!plain &&
            flat.map((o) => (
              <span key={o.value} className="popup-button-sizer" data-label={o.label} aria-hidden />
            ))}
        </span>
        {!plain && <ChevronsUpDown className="popup-button-chevron" size="0.846rem" strokeWidth={2.2} aria-hidden />}
      </button>
      {open && button && (
        <MenuPanel
          entries={entries}
          placement={{ below: button.getBoundingClientRect() }}
          label={props["aria-label"] ?? button.labels?.[0]?.textContent ?? undefined}
          mono={mono}
          opener={button}
          owner={button}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
