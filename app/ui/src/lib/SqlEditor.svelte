<script lang="ts">
  // cwt.4 — CodeMirror 6 T-SQL editor. It is the INPUT to the runner. Exposes
  // getText/getSelectionText/getRunTarget/focus via `bind:this` — the seam cwt.5
  // reads (App's Run button calls getRunTarget()). Cmd/Ctrl-Enter fires `onrun`.
  import { onMount } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { sql, MSSQL } from "@codemirror/lang-sql";
  import { toggleComment } from "@codemirror/commands";

  let {
    value = "", // INIT-ONLY doc text (per {#key activeId} remount); CM's doc is the
    // live source of truth. Not $bindable — edits flow OUT via `onchange`, not a bind.
    onchange, // called on every CM docChange with the new text (App wires it to setActiveContent)
    onrun, // fired by Cmd/Ctrl-Enter while CM holds focus (App wires it to run())
  }: { value?: string; onchange?: (text: string) => void; onrun?: () => void } = $props();

  let host = $state<HTMLDivElement>(); // bind:this on the container div
  let view: EditorView | undefined;
  let applyingExternal = false; // guards the value↔doc feedback loop

  const extensions = [
    // Explicit + high-precedence (prepended) so Mod-/ toggles comments regardless
    // of basicSetup internals. basicSetup's defaultKeymap also binds it; this
    // documents intent and guarantees the AC.
    keymap.of([
      { key: "Mod-/", run: toggleComment, preventDefault: true },
      // Cmd/Ctrl-Enter runs. High-precedence + preventDefault so it fires while
      // CM has focus (basicSetup doesn't bind Mod-Enter). Returns true to stop
      // further handlers even if `onrun` is unset.
      { key: "Mod-Enter", run: () => { onrun?.(); return true; }, preventDefault: true },
    ]),
    basicSetup, // line numbers, history, brackets, default keymap, highlighting, autocomplete
    sql({ dialect: MSSQL }), // T-SQL keywords + syntax highlighting
    EditorView.theme({
      "&": { height: "100%", fontSize: "13px" },
      ".cm-scroller": { overflow: "auto", fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" },
    }),
    // CM's own edits flow OUT via the onchange callback (one-directional data
    // flow: module → value init → CM → onchange → module → debounced save).
    EditorView.updateListener.of((u) => {
      if (u.docChanged) onchange?.(u.state.doc.toString());
    }),
  ];

  onMount(() => {
    // Create once (not in $effect, which would re-run on dep changes).
    view = new EditorView({
      parent: host!,
      state: EditorState.create({ doc: value, extensions }),
    });
    return () => view?.destroy();
  });

  // Reconcile an EXTERNAL set of `value` (e.g. loading a saved query in Phase 3)
  // back into CM, without echoing CM's own edits. The guard + the `===` no-op
  // check keep it loop-free. DORMANT this wave: cwt.8 swaps tabs by a
  // {#key activeId} remount (fresh CM per tab, so undo history/cursor are per-tab
  // — note: undo history resets on switch-away-and-back, each remount being a new
  // CM), never by re-setting `value` on a live instance, so `v === doc` always
  // holds here and this no-ops. Kept as defensive support for a future
  // set-without-remount caller (the Phase 3 load-a-query path).
  $effect(() => {
    const v = value;
    if (!view || applyingExternal) return;
    if (v === view.state.doc.toString()) return;
    applyingExternal = true;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
    applyingExternal = false;
  });

  // --- Public API (via bind:this) — the seam cwt.5 reads ---
  export function getText(): string {
    return view ? view.state.doc.toString() : value;
  }
  export function getSelectionText(): string {
    if (!view) return "";
    const { from, to } = view.state.selection.main;
    return view.state.sliceDoc(from, to);
  }
  export function getRunTarget(): { text: string; selection: string; line: number } {
    // What to run: the full doc text, the current selection (empty if none), and
    // the caret's 1-based line. core's batch_at_line uses `line` (line-based, not
    // byte offset) — CM's doc.lineAt(head).number matches Rust's split('\n')
    // index because CM normalizes all line breaks to `\n` in doc.toString().
    const text = getText();
    if (!view) return { text, selection: "", line: 1 };
    const { from, to, head } = view.state.selection.main;
    const selection = view.state.sliceDoc(from, to);
    const line = view.state.doc.lineAt(head).number;
    return { text, selection, line };
  }
  export function focus(): void {
    view?.focus();
  }
</script>

<div class="sql-editor" bind:this={host}></div>

<style>
  .sql-editor {
    height: 100%;
    overflow: hidden;
  }
</style>
