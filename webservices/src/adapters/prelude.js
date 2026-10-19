(function () {
  if (window.top !== window) return;
  var last = '', lastName = '', lastItems = null;
  // Tells the host the page's background colour so a dark page gets a dark title bar.
  var lastBg = '';
  function reportTheme() {
    var e = document.elementFromPoint(Math.floor(window.innerWidth / 2), Math.floor(window.innerHeight / 2));
    while (e) {
      var m = /rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?/.exec(getComputedStyle(e).backgroundColor);
      if (m && (m[4] === undefined || parseFloat(m[4]) > 0.5)) {
        var k = m[1] + ',' + m[2] + ',' + m[3];
        if (k !== lastBg) { lastBg = k; window.chrome.webview.postMessage('theme|' + k); }
        return;
      }
      e = e.parentElement;
    }
  }
  setInterval(reportTheme, 3000);
  window.__rt = {
    // Adapters call this with the unread total and whether a meeting is in progress.
    report: function (unread, meeting) {
      var k = unread + '|' + (meeting ? 1 : 0);
      if (k !== last) { last = k; window.chrome.webview.postMessage(k); }
    },
    // Reports whether the page shows a "sign in again" notice (session expired).
    watchSignin: function () {
      var q = "//text()[(contains(.,'You need to sign in') or contains(.,'session has expired') or contains(.,'Your session expired') or contains(.,'Sign in to continue') or contains(.,'sign in again'))]";
      var lastSign = null;
      setInterval(function () {
        var hit = false;
        // Being sent to the Microsoft sign-in page means the session is gone.
        if (window === window.top && /^login\.(microsoftonline|live)\.com$/.test(location.hostname)) hit = true;
        else try {
          var r = document.evaluate(q, document.body || document, null, 9, null).singleNodeValue;
          var el = r && r.parentElement;
          hit = !!el && !!(el.offsetParent || el.getClientRects().length > 0);
        } catch (e) {}
        if (hit !== lastSign) { lastSign = hit; window.chrome.webview.postMessage('signin|' + (hit ? 1 : 0)); }
      }, 3000);
    },
    log: function (t) { window.chrome.webview.postMessage('log|' + t); },
    // Display name for the service (e.g. the signed-in account).
    name: function (n) {
      if (n && n !== lastName) { lastName = n; window.chrome.webview.postMessage('name|' + n); }
    },
    // Unread mails as [{from, title, url, preview}] (tab/newline separated on the wire).
    items: function (rows) {
      var clean = function (s) { return String(s || '').replace(/[\t\r\n|]+/g, ' ').trim(); };
      var k = rows.slice(0, 20).map(function (r) { return [clean(r.from), clean(r.title), r.url, clean(r.preview).slice(0, 400)].join('\t'); }).join('\n');
      if (k !== lastItems) { lastItems = k; window.chrome.webview.postMessage('items|' + k); }
    },
    // Setting from the host (window.__opts), or the default when unset.
    opt: function (name, dflt) {
      var o = window.__opts;
      return o && o[name] !== undefined ? o[name] : dflt;
    },
    // Runs fn repeatedly at the configured poll interval (changes apply on the next pass).
    poll: function (fn, min) {
      (function next() { setTimeout(function () { try { fn(); } finally { next(); } }, Math.max(min || 0, window.__rt.opt('poll', 3000))); })();
    },
    title: function () {
      var m = /^\s*\((\d+)\)/.exec(document.title);
      return m ? parseInt(m[1], 10) : 0;
    },
  };
})();
