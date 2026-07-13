<script lang="ts">
  // A small animated loading row for the object tree, so a slow fetch reads as
  // "working" not "hung". (The tree fetches are slow mostly because each schema
  // query connects fresh — a full SQL login per expand; connection reuse is the
  // real speedup, tracked separately.)
  let { text = "Loading…" }: { text?: string } = $props();
</script>

<div class="loading">
  <span class="spinner" aria-hidden="true"></span>
  <span>{text}</span>
</div>

<style>
  .loading {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.15rem 0 0.15rem 0.7rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .spinner {
    width: 0.7rem;
    height: 0.7rem;
    flex: none;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  /* Respect reduced-motion: fall back to a static ring rather than spinning. */
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
</style>
