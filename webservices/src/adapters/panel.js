// Mail panel: shows only the open conversation by hiding everything else in Gmail's page.
if (window.top === window) {
  setInterval(function () {
    if (location.hostname !== 'mail.google.com' || document.getElementById('__panel') || !document.head) return;
    var s = document.createElement('style');
    s.id = '__panel';
    s.textContent =
      'body *{visibility:hidden!important}' +
      '.aeF,.aeF *{visibility:visible!important}' +
      '.aeF{position:fixed!important;left:0;top:0;width:100vw!important;height:100vh!important;' +
      'z-index:99999;background:#fff;overflow:auto!important}';
    document.head.appendChild(s);
  }, 300);
  // Wide (fixed-width) mails are shrunk to fit the panel instead of being cut off.
  setInterval(function () {
    var el = document.querySelector('.aeF');
    if (!el || !document.getElementById('__panel')) return;
    el.style.setProperty('zoom', '1', 'important');
    el.style.setProperty('width', '100vw', 'important');
    el.style.setProperty('height', '100vh', 'important');
    var need = el.scrollWidth, have = window.innerWidth;
    if (need > have + 2) {
      var z = Math.max(0.4, have / need);
      el.style.setProperty('zoom', String(z), 'important');
      el.style.setProperty('width', (100 / z) + 'vw', 'important');
      el.style.setProperty('height', (100 / z) + 'vh', 'important');
    }
  }, 500);
}
