// billz-6s0: a native tooltip carrying the FULL text, but only on rows that are
// actually truncated.
//
// Setting `title` unconditionally means every short row (`dbo.Users`, `Id`) pops
// an OS tooltip that just repeats what you can already read — new mouse-over
// noise on every row of a dense tree.
//
// Elision is a layout fact, so it can only be answered from the rendered box
// (`scrollWidth > clientWidth`). The obvious implementation is a ResizeObserver
// per label, but a tree with a few hundred columns expanded would then carry a few
// hundred observers for a hover affordance. Instead this resolves lazily on
// pointerenter: layout is settled by then, it costs nothing until the pointer
// actually arrives, and the attribute lands well before the browser's ~1s tooltip
// delay elapses.
export function elidedTitle(node: HTMLElement, text: string) {
  let full = text;

  function sync() {
    // +1 tolerance: sub-pixel text metrics can leave scrollWidth a hair over
    // clientWidth on a label that renders with no visible ellipsis.
    if (node.scrollWidth > node.clientWidth + 1) node.setAttribute("title", full);
    else node.removeAttribute("title");
  }

  node.addEventListener("pointerenter", sync);

  return {
    update(next: string) {
      full = next;
      // Only correct a title that's already showing; otherwise wait for the next
      // hover rather than measuring on every reactive update.
      if (node.hasAttribute("title")) sync();
    },
    destroy() {
      node.removeEventListener("pointerenter", sync);
    },
  };
}
