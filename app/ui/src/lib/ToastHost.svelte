<script lang="ts">
  // Toast overlay (billz-086). Mounted ONCE, from App.svelte. Fixed-position, so
  // it's independent of the 3-column app grid.
  //
  // Scope: transient app-level events only. Query execution output stays in the
  // Messages pane — see the note at the top of toastLogic.ts.
  import { tick } from "svelte";
  import { AlertCircle, Check, Info, X } from "./icons";
  import { dismiss, toasts } from "./toasts.svelte";
  import { isAssertive, type ToastKind } from "./toastLogic";

  // Annotated (not inferred) so adding a ToastKind fails HERE, on the missing
  // entry, rather than as an index error down in the markup.
  const ICONS: Record<ToastKind, typeof Info> = {
    success: Check,
    error: AlertCircle,
    info: Info,
  };

  let host = $state<HTMLDivElement>();

  /**
   * Dismiss from the ✕, keeping keyboard focus inside the stack.
   *
   * Unmounting the focused button drops focus to <body>, so the user's next Tab
   * restarts from the top of the document. Only re-aims focus when the button
   * actually had it — WebKit doesn't focus a button on mouse-down, so a click
   * dismissal correctly leaves focus where it was.
   */
  async function dismissFromButton(event: Event, id: string, index: number) {
    const hadFocus = event.currentTarget === document.activeElement;
    dismiss(id);
    if (!hadFocus) return;
    await tick();
    const buttons = host?.querySelectorAll<HTMLButtonElement>(".close");
    if (buttons === undefined || buttons.length === 0) return;
    buttons[Math.min(index, buttons.length - 1)].focus();
  }
</script>

<!-- aria-atomic=false so a new toast announces on its own rather than re-reading
     the whole stack. The region stays mounted (never {#if}-gated) — screen readers
     only pick up additions to a live region that already existed. -->
<div class="host" bind:this={host} aria-live="polite" aria-atomic="false">
  {#each toasts.list as toast, i (toast.id)}
    {@const Icon = ICONS[toast.kind]}
    <div class="toast {toast.kind}" role={isAssertive(toast.kind) ? "alert" : undefined}>
      <Icon size={15} />
      <span class="text">{toast.text}</span>
      <button
        class="close"
        aria-label="Dismiss notification"
        onclick={(e) => dismissFromButton(e, toast.id, i)}
      >
        <X size={13} />
      </button>
    </div>
  {/each}
</div>

<style>
  .host {
    position: fixed;
    bottom: var(--sp-4);
    right: var(--sp-4);
    /* Above the modal ladder (context menus 40/41, PasswordPrompt 50/51) so a save
       FAILURE stays readable over the dialog that caused it. */
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    /* Uniform width rather than shrink-to-fit: a stack whose left edge is ragged
       reads as sloppy, and a one-line toast next to a wrapped one is the common
       case. The container never blocks clicks on the app behind it — only the
       toasts themselves are interactive. */
    width: min(24rem, calc(100vw - 2 * var(--sp-4)));
    align-items: stretch;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--raised);
    border: 1px solid var(--border-strong);
    /* Kind is carried by a left rule + the icon rather than a saturated fill —
       keeps it legible in both themes without a new palette. */
    border-left: 3px solid var(--border-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-md);
    font-family: var(--font-ui);
    font-size: var(--fs-sm);
    color: var(--text);
    /* CSS transition on the motion tokens, NOT a Svelte transition directive:
       app.css zeroes --dur/--dur-fast under prefers-reduced-motion, and a
       JS-driven transition would bypass that guard and animate anyway. */
    animation: toast-in var(--dur) var(--ease);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(0.4rem);
    }
  }
  .toast.success {
    border-left-color: var(--ok);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .toast.info {
    border-left-color: var(--brand);
  }
  /* Child combinator, not a descendant selector: the kind icon is a DIRECT child
     of .toast, whereas the ✕ is nested in the close button — where it is also a
     first child, so `svg:first-child` would tint it the kind color too. */
  .toast.success > :global(svg) {
    color: var(--ok);
  }
  .toast.error > :global(svg) {
    color: var(--danger);
  }
  .toast.info > :global(svg) {
    color: var(--brand);
  }
  /* Child combinator here too, for the same reason as the color rules above: the
     1px optical nudge lines the KIND icon up with the first line of text, and must
     not also shift the ✕ inside the (already centered) close button. */
  .toast > :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .close :global(svg) {
    flex: none;
  }
  .text {
    /* Long SQL errors wrap instead of stretching the toast off-screen... */
    overflow-wrap: anywhere;
    /* ...and are clamped vertically for the same reason. The stack is anchored to
       `bottom` and grows UP, so a toast taller than the viewport pushes its own ✕
       (top-right, under `align-items: flex-start`) above y=0 — and `position:
       fixed` means nothing can scroll it back. Combined with sticky errors that
       makes a long message permanently undismissable. ~6 lines, then scroll. */
    max-height: 8.4em;
    overflow-y: auto;
  }
  .close {
    flex: none;
    display: flex;
    align-items: center;
    /* auto, not a fixed gap: pushes ✕ to the right edge now that the stack is a
       uniform width rather than shrink-to-fit. */
    margin-left: auto;
    padding: 2px;
    border: none;
    border-radius: var(--r-sm);
    background: none;
    color: var(--muted);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .close:hover {
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: var(--text);
  }
</style>
