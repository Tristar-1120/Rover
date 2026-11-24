// ==UserScript==
// @name        Dark mode for websites (Rover built-in)
// @description Darkens light websites while leaving already-dark sites, photos, videos, and large canvases intact.
// @version     2.0.0
// @match       *://*/*
// @run-at      document-start
// ==/UserScript==
(() => {
  const root = document.documentElement;
  if (!root) return;
  window.__roverDarkSync = (on) => {
    if (on) {
      root.setAttribute('data-rover-dark', '1');
      root.removeAttribute('data-rover-dark-paused');
    } else {
      root.removeAttribute('data-rover-dark');
      root.setAttribute('data-rover-dark-paused', '1');
    }
  };
  window.__roverDarkSync(true);
})();
