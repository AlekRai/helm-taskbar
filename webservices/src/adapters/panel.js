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
}
