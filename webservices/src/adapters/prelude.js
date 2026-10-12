(function () {
  if (window.top !== window) return;
  var last = '', lastName = '', lastItems = null;
  window.__rt = {
    // Adapters call this with the unread total and whether a meeting is in progress.
    report: function (unread, meeting) {
      var k = unread + '|' + (meeting ? 1 : 0);
      if (k !== last) { last = k; window.chrome.webview.postMessage(k); }
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
