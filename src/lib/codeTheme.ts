import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

/** CodeMirror colors from the scheme's `--code-*` tokens, so the editor follows every scheme without a
 *  theme per scheme. `dark` only picks CodeMirror's built-in base (search panel, tooltips). */
export function codeTheme(dark: boolean) {
  return [
    EditorView.theme(
      {
        "&": { color: "var(--code-name)", backgroundColor: "var(--bg)" },
        ".cm-content": { caretColor: "var(--code-cursor)", fontFamily: "var(--font-mono)" },
        ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--code-cursor)" },
        "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
          backgroundColor: "var(--code-selection)",
        },
        ".cm-activeLine": { backgroundColor: "var(--code-active-line)" },
        ".cm-selectionMatch, .cm-searchMatch": { backgroundColor: "var(--code-match)" },
        ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--code-gutter)", border: "none" },
        ".cm-activeLineGutter": { backgroundColor: "var(--code-active-line)", color: "var(--text)" },
        ".cm-placeholder": { color: "var(--o3)" },
        ".cm-tooltip": { backgroundColor: "var(--pop)", border: "1px solid var(--field-line)", color: "var(--text)" },
        ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--sel)", color: "var(--text)" },
      },
      { dark },
    ),
    syntaxHighlighting(
      HighlightStyle.define([
        { tag: [t.keyword, t.operatorKeyword, t.modifier], color: "var(--code-keyword)" },
        { tag: [t.string, t.special(t.string)], color: "var(--code-string)" },
        { tag: [t.number, t.bool, t.null, t.atom], color: "var(--code-number)" },
        { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--code-comment)", fontStyle: "italic" },
        { tag: [t.function(t.variableName), t.standard(t.name)], color: "var(--code-function)" },
        { tag: [t.typeName, t.className], color: "var(--code-type)" },
        { tag: [t.operator, t.punctuation], color: "var(--code-operator)" },
        { tag: [t.special(t.name), t.quote], color: "var(--code-type)" },
        { tag: t.invalid, color: "var(--code-invalid)" },
      ]),
    ),
  ];
}
