<script lang="ts">
  // Destructive-action confirmation (billz-rvg). Third in the inline-dialog family
  // established by PasswordPrompt.svelte and generalized by NameDialog.svelte.
  //
  // billz-9ug — THE RECORDED FINDING, since this is the file that resolves it:
  // NameDialog documents that `window.prompt` is unreliable in the Tauri v2
  // WKWebView, but the codebase went on depending on `window.confirm` in two
  // places (the saved-query delete and the connection delete). That asserted one
  // native dialog was untrustworthy while betting on its sibling. Rather than
  // prove `confirm` reliable in this webview — a DMG-only test whose answer is one
  // Tauri/WebKit bump from going stale — both call sites now route here. The rule
  // is uniform and needs no re-litigating: THIS APP OWNS ITS DIALOGS.
  //
  // Kept generic (title/message/confirmLabel are props) for the same reason
  // NameDialog is: two bespoke confirmation UIs are how two confirmation flows
  // drift apart.
  import { AlertCircle } from "./icons";

  let {
    title,
    message,
    confirmLabel = "Delete",
    onconfirm,
    oncancel,
  }: {
    title: string;
    message: string;
    confirmLabel?: string;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();

  let cancelButton = $state<HTMLButtonElement>();

  // Focus CANCEL, not confirm — the one deliberate divergence from the
  // `window.confirm` this replaces (which focuses OK). Both call sites delete
  // something with no undo, so ⏎ on open cancels and confirming costs a click or
  // a Tab. Native parity would have preserved a hazard we are rewriting anyway;
  // the gesture that destroys data should cost more than a reflex.
  //
  // No <form> here (there's no input): focusing a real <button> is what makes
  // ⏎/Space activate it, so the keyboard path needs nothing else.
  $effect(() => {
    cancelButton?.focus();
  });
</script>

<!-- Escape cancels (mirrors NameDialog / PasswordPrompt / TableNode's menu).

     TODO(billz-ppw): like every other component-owned dialog here, App's
     window-level ⌘S handler can't see this one — it guards only App's OWN modal
     state — so ⌘S while this is open can mount a NameDialog over it, and one
     Escape closes both (each instance registers its own listener). Pre-existing
     and already live via SavedQueryLibrary's rename dialog; this adds two more
     instances of that category, not a new one. The fix is a shared "is any modal
     open" signal in App.svelte, which is O(1) in the number of dialogs. -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") oncancel();
  }}
/>

<!-- Button backdrop (not a static div) so svelte-check a11y stays clean — same
     pattern as NameDialog.svelte / PasswordPrompt.svelte. -->
<button class="backdrop" aria-label="Cancel" onclick={oncancel}></button>
<div class="modal" role="dialog" aria-modal="true" aria-label={title}>
  <h3><AlertCircle size={16} /> {title}</h3>
  <p class="message">{message}</p>
  <div class="actions">
    <button type="button" class="danger" onclick={onconfirm}>{confirmLabel}</button>
    <button type="button" bind:this={cancelButton} onclick={oncancel}>Cancel</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: var(--scrim);
    border: none;
    padding: 0;
    cursor: default;
  }
  .modal {
    position: fixed;
    top: 30%;
    left: 50%;
    transform: translateX(-50%);
    z-index: 51;
    background: var(--raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-xl);
    padding: var(--sp-4) var(--sp-5);
    box-shadow: var(--shadow-md);
    min-width: 20rem;
    max-width: min(28rem, calc(100vw - 2 * var(--sp-4)));
    font-family: var(--font-ui);
    color: var(--text);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    margin: 0 0 0.6rem;
    font-size: var(--fs-md);
    color: var(--text);
  }
  /* The attention icon carries the destructive read; the header text stays in
     --text so the dialog doesn't shout. */
  h3 :global(svg) {
    color: var(--danger);
    flex: none;
  }
  .message {
    margin: 0 0 0.8rem;
    font-size: var(--fs-sm);
    color: var(--muted);
    /* A saved query's "name" can be a whole pasted statement (see renameSeed) —
       wrap it rather than stretching the dialog off-screen. */
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: var(--sp-1);
    justify-content: flex-end;
  }
  button {
    font-size: var(--fs-sm);
    cursor: pointer;
    padding: var(--sp-1) var(--sp-3);
    border-radius: var(--r-sm);
    border: 1px solid var(--border-strong);
    background: var(--raised);
    color: var(--text);
    font-family: inherit;
    transition: background var(--dur-fast) var(--ease);
  }
  /* The destructive action reads as destructive. Sibling dialogs tint their
     submit with --accent; this one is not a happy path.
     --accent-fg is reused rather than minting a --danger-fg: it is the palette's
     "text on a saturated fill" token and it inverts with the theme exactly as
     --danger does (light: #fff on #dc2626; dark: #0b0f0e on #f87171), so both
     directions land on the readable pairing. */
  .danger {
    background: var(--danger);
    color: var(--accent-fg);
    border-color: var(--danger);
  }
</style>
