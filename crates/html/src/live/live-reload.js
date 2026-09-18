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

  // IS THIS PAGE SAFE TO MORPH? Reusing a script tag is exactly what keeps one
  // EventSource open above, and it is also what makes a morph WRONG for any
  // page whose scripts built state the compiled HTML does not carry. Refetched
  // page bytes are always the PRE-HYDRATION build output: a widget's input is
  // empty, its buttons are unpressed, its rows are in build order, and the
  // attribute its stylesheet keys off says "not ready". Morphing that over a
  // live island reverts every one of those AND does not re-run the script that
  // would set them again — the island is left drawn as though it had never
  // booted, which is worse than the reload it replaced.
  //
  // So a script must DECLARE that it can rebuild its own state, by shipping
  // under a package that sets `js_rehydrate = true` and pushing a callback
  // onto `window.__rheoRehydrate` (see `docs/contract.md`). rheo renders a
  // declaring script with `data-rheo-rehydrate`, and one undeclared script is
  // enough to disqualify the page: a page is a single DOM, and morphing it for
  // the sake of the widgets that cope would break the ones that do not.
  //
  // PER SCRIPT rather than per page, and read off the DOM rather than counted
  // out of `__rheoRehydrate`, because the failure being avoided is the MIXED
  // page — one migrated package and one not. A non-empty hook list says
  // somebody can rehydrate, never that everybody can.
  const morphable = () => {
    for (const script of document.querySelectorAll('script[src]')) {
      // This file, injected into every served page by `inject_live_reload_script`.
      if (script.hasAttribute('data-rheo-live')) continue;
      if (!script.hasAttribute('data-rheo-rehydrate')) return false;
    }
    return true;
  };

  // A rebuild that touched only .typ sources: the page bytes are the whole
  // delta, so morphing them in preserves scroll, focus, selection, open
  // <details>, and media playback that a navigation would discard.
  source.addEventListener('morph', async () => {
    if (!morphable()) return location.reload();
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
      // CHECKED AGAIN, because the survey above ran against the OLD DOM. An
      // edit that puts a new widget on the page brings its scripts in with it,
      // and a <script> the morph INSERTS does execute — so a newly arrived
      // island boots by itself. One that arrives undeclared, though, was never
      // covered by the decision to morph, and the page is now the mixed case
      // that decision existed to avoid.
      if (!morphable()) return location.reload();
      // THE HOOKS RUN AFTER THE MORPH, never before: they read the DOM they are
      // rebuilding state onto, and the DOM they need is the one that just
      // landed. Registered with `??=` at the far end, so this list exists
      // whether or not this file loaded first.
      for (const rehydrate of window.__rheoRehydrate ?? []) rehydrate();
    } catch (e) {
      // COVERS THE HOOKS TOO. A hook that throws has left its island in an
      // unknown state — half re-wired, or not at all — and a reload is the one
      // recovery that does not depend on knowing which.
      console.warn('rheo: morph failed, reloading', e);
      location.reload();
    }
  });
}
