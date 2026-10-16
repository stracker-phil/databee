import { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { useBindingLabel } from "../commands";
import { MySQL, sql } from "@codemirror/lang-sql";
import { Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { codeTheme } from "../lib/codeTheme";
import { statementAt } from "../lib/sql";

interface Props {
  value: string;
  onChange: (value: string) => void;
  schema: Record<string, string[]>;
  /** The applied scheme is dark; colors come from its `--code-*` tokens. */
  dark: boolean;
  onView?: (view: EditorView) => void;
}

const EXAMPLE_QUERY = "SELECT * FROM wp_options WHERE autoload = 'yes'";

export function currentStatement(view: EditorView): string {
  const { state } = view;
  const sel = state.selection.main;
  const range = statementAt(state.doc.toString(), sel.head, { from: sel.from, to: sel.to });
  return range ? state.sliceDoc(range.from, range.to) : "";
}

export function QueryEditor({ value, onChange, schema, dark, onView }: Props) {
  const runKey = useBindingLabel("editor.run");
  const placeholder = runKey ? `${EXAMPLE_QUERY}   ${runKey} runs the statement at the cursor` : EXAMPLE_QUERY;

  const extensions = useMemo(
    () => [
      sql({ dialect: MySQL, schema, upperCaseKeywords: true }),
      // ⌘↵ / ⇧⌘↵ are app commands (editor.run / editor.runAll); keep CodeMirror from inserting a newline.
      Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => true }, { key: "Shift-Mod-Enter", run: () => true }])),
      EditorView.lineWrapping,
      codeTheme(dark),
    ],
    [schema, dark],
  );

  return (
    <CodeMirror
      className="editor"
      data-stage="editor"
      data-stage-label="SQL editor"
      data-stage-role="field"
      value={value}
      height="100%"
      theme="none"
      autoFocus
      placeholder={placeholder}
      basicSetup={{ foldGutter: false, highlightActiveLineGutter: true }}
      extensions={extensions}
      onChange={onChange}
      onCreateEditor={(view) => onView?.(view)}
    />
  );
}
