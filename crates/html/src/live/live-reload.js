// Appended after the vendored Idiomorph bundle and served as one file, so the
// morph algorithm is in scope by the time this runs.

// A body morph reuses the identical <script src> tag rather than re-adding it,
// but a positional mismatch would re-execute this file and open a second
// EventSource. One connection per document, whatever the morph decides.
if (!window.__rheoLive) {
  window.__rheoLive = true;

  const source = new EventSource('/events');

  // The server greets every subscription. A second greeting means this page
  // resubscribed, so the rebuilds broadcast while it was away are lost and the
  // DOM's baseline is unknown — navigate rather than morph against it.
  let greeted = false;
  source.addEventListener('hello', () => {
    if (greeted) location.reload();
    greeted = true;
  });

  source.addEventListener('reload', () => location.reload());

  // A rebuild that touched only .typ sources: the page bytes are the whole
  // delta, so morphing them in preserves scroll, focus, selection, open
  // <details>, and media playback that a navigation would discard.
  source.addEventListener('morph', async () => {
    try {
      const response = await fetch(location.href, { cache: 'no-store' });
      if (!response.ok) return location.reload();
      Idiomorph.morph(document.documentElement, await response.text(), {
        // A .typ edit still reaches <head> — a #set document(title:), a
        // <rheo-head> wrapper, a marrow-minted .rheo/head.html — so head is
        // reconciled rather than skipped.
        head: { style: 'merge' },
        // Compiled HTML carries no typed value, so syncing one over the
        // focused field would erase what the reader is in the middle of
        // writing.
        ignoreActiveValue: true,
      });
    } catch (e) {
      console.warn('rheo: morph failed, reloading', e);
      location.reload();
    }
  });
}
