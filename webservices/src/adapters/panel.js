// Mail panel: shows only the open conversation by hiding everything else in Gmail's page.
// The element holding just the open mail, per site.
function panelSel() {
  if (location.hostname === 'mail.google.com') return '.aeF';
  if (/outlook\.(cloud\.microsoft|office\.com|live\.com)$/.test(location.hostname)) return '#ReadingPaneContainerId';
  if (/(^|\.)messenger\.com$/.test(location.hostname)) return '[role=main]';
  if (/(^|\.)teams\.(cloud\.)?microsoft(\.com)?$/.test(location.hostname)) return '[data-tid="app-layout-area--main"]';
  return '';
}
// First opaque background at or above the element, so dark pages keep their dark panel.
function panelBg(el) {
  for (; el; el = el.parentElement) {
    var m = /rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?/.exec(getComputedStyle(el).backgroundColor);
    if (m && (m[4] === undefined || parseFloat(m[4]) > 0.5)) return 
  // Deep links first show a download page; pick the web app.
  setInterval(function () {
    var b = Array.from(document.querySelectorAll('button, a')).find(function (x) { return /^use the web app instead$/i.test(x.innerText.trim()); });
    if (b) b.click();
  }, 500);
  setInterval(function () {
    var sel = panelSel();
    var target = sel && document.querySelector(sel);
    if (!target || document.getElementById('__panel') || !document.head) return;
    var s = document.createElement('style');
    s.id = '__panel';
    s.textContent =
      'body *{visibility:hidden!important}' +
      sel + ',' + sel + ' *{visibility:visible!important}' +
      sel + '{position:fixed!important;left:0;top:0;width:100vw!important;height:100vh!important;' +
      'z-index:99999;background:' + panelBg(target) + ';overflow:auto!important}';
    document.head.appendChild(s);
  }, 300);
  // Mails wider than the panel ask the host to widen the window.
  setInterval(function () {
    var sel = panelSel();
    var el = sel && document.querySelector(sel);
    if (!el || !document.getElementById('__panel')) return;
    var have = window.innerWidth, need = el.scrollWidth;
    var all = el.querySelectorAll('*');
    for (var i = 0; i < all.length; i++) {
      var e = all[i];
      if (e.clientWidth > 5 && e.scrollWidth > e.clientWidth + 2) {
        var ox = getComputedStyle(e).overflowX;
        // The element keeps its margins, so the window grows by exactly the hidden part.
        if (ox === 'auto' || ox === 'scroll') need = Math.max(need, have + e.scrollWidth - e.clientWidth);
      }
    }
    if (need > have + 2) window.chrome.webview.postMessage('panelw|' + Math.ceil(need * window.devicePixelRatio));
  }, 400);
}
