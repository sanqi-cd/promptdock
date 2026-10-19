/**
 * Whether a modal/overlay is currently rendered.
 *
 * Overlays are plain divs (not the shared Dialog primitive), and their
 * shortcuts must not leak to the global key handlers. Each overlay marks its
 * root with `data-modal-open="true"`; this keeps the check framework-agnostic
 * and avoids threading overlay state through every component.
 */
export function isOverlayOpen(): boolean {
  return document.querySelector('[data-modal-open="true"]') !== null;
}
