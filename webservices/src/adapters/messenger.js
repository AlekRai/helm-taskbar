// Unread count = the "Chats · N unread" label of the chat list, or the rows marked "Unread message".
if (window.top === window) {
  setInterval(function () {
    if (!document.querySelector('[role="grid"], [aria-label^="Chats"]')) return;
    var tab = document.querySelector('a[aria-label^="Chats"][aria-label*="unread"]');
    var m = tab && /(\d+)\s+unread/i.exec(tab.getAttribute('aria-label'));
    if (m) { window.__rt.report(parseInt(m[1], 10), false); return; }
    var rows = Array.from(document.querySelectorAll('[role="grid"] a')).filter(function (a) {
      return /Unread message/i.test(a.innerText);
    });
    window.__rt.report(rows.length, false);
  }, 2000);
}
