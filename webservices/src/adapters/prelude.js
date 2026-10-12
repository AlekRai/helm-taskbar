(function () {
  if (window.top !== window) return;
  var last = '';
  window.__rt = {
    // Adapters call this with the unread total and whether a meeting is in progress.
    report: function (unread, meeting) {
      var k = unread + '|' + (meeting ? 1 : 0);
      if (k !== last) { last = k; window.chrome.webview.postMessage(k); }
    },
    log: function (t) { window.chrome.webview.postMessage('log|' + t); },
    title: function () {
      var m = /^\s*\((\d+)\)/.exec(document.title);
      return m ? parseInt(m[1], 10) : 0;
    },
  };
})();
