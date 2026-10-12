// Unread count = the Activity and Chat badges in the app bar (summed). Meeting detection is still TODO.
if (window.top === window) {
  setInterval(function () {
    var bar = document.querySelector('[data-tid="app-bar-wrapper"]');
    if (!bar) return;
    var total = 0;
    bar.querySelectorAll('*').forEach(function (e) {
      if (e.children.length || !/^\d+\+?$/.test(e.textContent.trim())) return;
      var near = Array.from(e.parentElement.parentElement.children).concat(Array.from(e.parentElement.children));
      var rail = near.some(function (s) { return /^(Activity|Chat) \(/.test(s.getAttribute('aria-label') || ''); });
      if (rail) total += parseInt(e.textContent, 10);
    });
    window.__rt.report(total, false);
  }, 2000);
}
