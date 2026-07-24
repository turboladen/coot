<script lang="ts">
  // Toast overlay (billz-086). Mounted ONCE, from App.svelte. Fixed-position, so
  // it's independent of the 3-column app grid.
  //
  // Scope: transient app-level events only. Query execution output stays in the
  // Messages pane — see the note at the top of toastLogic.ts.
  import { AlertCircle, Check, Info, X } from "./icons";
  import { dismiss, toasts } from "./toasts.svelte";
  import type { ToastKind } from "./toastLogic";

  const ICONS = { success: Check, error: AlertCircle, info: Info };

  // Errors announce assertively (role="alert"); the container's aria-live="polite"
  // covers the rest. This is the app's first live region.
  function isAssertive(kind: ToastKind): boolean {
    return kind === "error";
  }
</script>

<!-- aria-atomic=false so a new toast announces on its own rather than re-reading
     the whole stack. The region stays mounted (never {#if}-gated) — screen readers
     only pick up additions to a live region that already existed. -->
<div class="host" aria-live="polite" aria-atomic="false">
  {#each toasts.list as toast (toast.id)}
    {@const Icon = ICONS[toast.kind]}
    <div class="toast {toast.kind}" role={isAssertive(toast.kind) ? "alert" : undefined}>
      <Icon size={15} />
      <span class="text">{toast.text}</span>
      <button class="close" aria-label="Dismiss notification" onclick={() => dismiss(toast.id)}>
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
    /* The container spans no more than it needs, and never blocks clicks on the
       app behind it — only the toasts themselves are interactive. */
    align-items: flex-end;
    pointer-events: none;
    max-width: min(24rem, calc(100vw - 2 * var(--sp-4)));
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
  .toast.success :global(svg:first-child) {
    color: var(--ok);
  }
  .toast.error :global(svg:first-child) {
    color: var(--danger);
  }
  .toast.info :global(svg:first-child) {
    color: var(--brand);
  }
  .toast :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .text {
    /* Long SQL errors wrap instead of stretching the toast off-screen. */
    overflow-wrap: anywhere;
  }
  .close {
    flex: none;
    display: flex;
    align-items: center;
    margin-left: var(--sp-1);
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
