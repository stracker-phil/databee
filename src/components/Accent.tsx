import { ACCENTS } from "../api";

/** One swatch per accent hue; `value` is pressed (`null`: none is). Stage anchors are `<stage>:<accent>` in list `<stage>`. */
export function AccentSwatches({ value, stage, small, onChange }: {
  value: string | null;
  stage: string;
  small?: boolean;
  onChange: (accent: string) => void;
}) {
  return (
    <div className={small ? "accent-swatches small" : "accent-swatches"} role="group" aria-label="Accent">
      {ACCENTS.map((a) => (
        <button
          key={a}
          type="button"
          aria-label={`Accent ${a}`}
          aria-pressed={value === a}
          title={a}
          data-stage={`${stage}:${a}`}
          data-stage-list={stage}
          style={{ background: `var(--a-${a})` }}
          onClick={() => onChange(a)}
        />
      ))}
    </div>
  );
}

/** A small dot in an accent hue, before the name of a database with its own accent. */
export function AccentDot({ accent }: { accent: string }) {
  return <span className="accent-dot" data-accent={accent} style={{ background: `var(--a-${accent})` }} aria-hidden="true" />;
}
