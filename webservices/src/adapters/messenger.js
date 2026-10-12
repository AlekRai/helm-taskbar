// First guess: the page title count. Refined once the real page structure is inspected.
if (window.top === window) {
  setInterval(function () { window.__rt.report(window.__rt.title(), false); }, 2000);
}
