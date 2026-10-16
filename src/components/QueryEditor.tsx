import { useMemo, useRef } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { MySQL, sql } from "@codemirror/lang-sql";
import { Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { statementAt } from "../lib/sql";

interface Props {
  value: string;
  onChange: (value: string) => void;
  schema: Record<string, string[]>;
  theme: "light" | "dark";
  onView?: (view: EditorView) => void;
}

export function currentStatement(view: EditorView): string {
  const { state } = view;
  const sel = state.selection.main;
  const range = statementAt(state.doc.toString(), sel.head, { from: sel.from, to: sel.to });
  return range ? state.sliceDoc(range.from, range.to) : "";
}

export function QueryEditor({ value, onChange, schema, theme, onView }: Props) {
  const ref = useRef<ReactCodeMirrorRef>(null);

  const extensions = useMemo(
    () => [
      sql({ dialect: MySQL, schema, upperCaseKeywords: true }),
      // ⌘↵ / ⇧⌘↵ are app commands (editor.run / editor.runAll); keep CodeMirror from inserting a newline.
      Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => true }, { key: "Shift-Mod-Enter", run: () => true }])),
      EditorView.lineWrapping,
    ],
    [schema],
  );

  return (
    <CodeMirror
      ref={ref}
      className="editor"
      data-stage="editor"
      data-stage-label="SQL editor"
      data-stage-role="field"
      value={value}
      height="100%"
      theme={theme}
      autoFocus
      placeholder="SELECT * FROM wp_options WHERE autoload = 'yes'  —  ⌘↵ runs the statement at the cursor"
      basicSetup={{ foldGutter: false, highlightActiveLineGutter: true }}
      extensions={extensions}
      onChange={onChange}
      onCreateEditor={(view) => onView?.(view)}
    />
  );
}
