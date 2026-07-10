<script lang="ts">
  // cwt.4 — CodeMirror 6 T-SQL editor. It is the INPUT to the runner. Exposes
  // getText/getSelectionText/getRunText/focus via `bind:this` — the seam cwt.5
  // depends on (Run button reads getRunText()). No run-wiring lives here.
  import { onMount } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { sql, MSSQL } from "@codemirror/lang-sql";
  import { toggleComment } from "@codemirror/commands";

  let {
    value = $bindable(""), // two-way document text; source of truth is CM's doc
  }: { value?: string } = $props();

  let host = $state<HTMLDivElement>(); // bind:this on the container div
  let view: EditorView | undefined;
  let applyingExternal = false; // guards the value↔doc feedback loop

  const extensions = [
    // Explicit + high-precedence (prepended) so Mod-/ toggles comments regardless
    // of basicSetup internals. basicSetup's defaultKeymap also binds it; this
    // documents intent and guarantees the AC.
    keymap.of([{ key: "Mod-/", run: toggleComment, preventDefault: true }]),
    basicSetup, // line numbers, history, brackets, default keymap, highlighting, autocomplete
    sql({ dialect: MSSQL }), // T-SQL keywords + syntax highlighting
    EditorView.theme({
      "&": { height: "100%", fontSize: "13px" },
      ".cm-scroller": { overflow: "auto", fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" },
    }),
    // CM's own edits flow out to the bindable `value`.
    EditorView.updateListener.of((u) => {
      if (u.docChanged) value = u.state.doc.toString();
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
  // check keep it loop-free.
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
  export function getRunText(): string {
    // "run the selection, else the whole batch" — text half only; splitting on
    // GO and calling run_sql is cwt.5's job.
    return getSelectionText() || getText();
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
