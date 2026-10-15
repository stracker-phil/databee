import { useMemo, useRef } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { MySQL, sql } from "@codemirror/lang-sql";
import { Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { statementAt } from "../lib/sql";

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** Called with the statement under the cursor (or the selection). */
  onRun: (sql: string) => void;
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

export function QueryEditor({ value, onChange, onRun, schema, theme, onView }: Props) {
  const ref = useRef<ReactCodeMirrorRef>(null);
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;

  const extensions = useMemo(
    () => [
      sql({ dialect: MySQL, schema, upperCaseKeywords: true }),
      Prec.highest(
        keymap.of([
          {
            key: "Mod-Enter",
            run: (view) => {
              onRunRef.current(currentStatement(view));
              return true;
            },
          },
          {
            key: "Shift-Mod-Enter",
            run: (view) => {
              onRunRef.current(view.state.doc.toString());
              return true;
            },
          },
        ]),
      ),
      EditorView.lineWrapping,
    ],
    [schema],
  );

  return (
    <CodeMirror
      ref={ref}
      className="editor"
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
