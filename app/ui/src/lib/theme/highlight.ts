import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

// One theme, driven by the app's CSS variables — so the editor flips with
// light/dark automatically (EditorView.theme values are plain CSS strings, so
// `var(--x)` resolves against the document like any other CSS).
const style = HighlightStyle.define([
  { tag: [t.keyword, t.modifier, t.operatorKeyword], color: "var(--syn-kw)", fontWeight: "500" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-fn)" },
  { tag: [t.string, t.special(t.string)], color: "var(--syn-str)" },
  { tag: [t.number, t.bool, t.null], color: "var(--syn-num)" },
  { tag: [t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.variableName, t.propertyName], color: "var(--syn-var)" },
]);

export const cootHighlight = syntaxHighlighting(style);

export const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px", backgroundColor: "var(--raised)", color: "var(--text)" },
  ".cm-scroller": { overflow: "auto", fontFamily: "var(--font-mono)", lineHeight: "1.6" },
  ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--faint)", border: "none", borderRight: "1px solid var(--border)" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--brand) 6%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--brand) 8%, transparent)", color: "var(--muted)" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "var(--accent)" },
  "&.cm-focused .cm-selectionBackground, ::selection": { backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent)" },
  ".cm-selectionBackground": { backgroundColor: "color-mix(in srgb, var(--accent) 14%, transparent)" },
});
