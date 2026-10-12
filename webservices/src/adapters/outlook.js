// Unread count = sum of "(N unread)" over every folder except Junk Email and Deleted Items.
if (window.top === window) {
  var SKIP = ['junk email', 'deleted items'];
  setInterval(function () {
    var items = document.querySelectorAll('[role="treeitem"][data-folder-name]');
    if (!items.length) return;
    var total = 0;
    items.forEach(function (e) {
      if (SKIP.indexOf(e.getAttribute('data-folder-name')) >= 0) return;
      var m = /\((\d[\d,]*) unread\)/.exec(e.getAttribute('title') || '');
      if (m) total += parseInt(m[1].replace(/,/g, ''), 10);
    });
    window.__rt.report(total, false);
  }, 2000);

  // Unread rows of the open Inbox list (sender, subject, preview) with a deep link to each conversation.
  function loadMails() {
    var sel = document.querySelector('[role="treeitem"][aria-selected="true"][data-folder-name]');
    if (!sel || sel.getAttribute('data-folder-name') !== 'inbox') return;
    var all = document.querySelectorAll('[role="listbox"] [role="option"][data-convid]');
    if (!all.length) return;
    var rows = [];
    all.forEach(function (e) {
      if (!/^Unread\b/.test(e.getAttribute('aria-label') || '')) return;
      var lines = e.innerText.split('\n').map(function (s) { return s.trim(); }).filter(Boolean);
      rows.push({
        from: lines[0] || '',
        title: lines[1] || '',
        preview: lines.slice(3).join(' '),
        url: location.origin + '/mail/inbox/id/' + encodeURIComponent(e.getAttribute('data-convid')),
      });
    });
    window.__rt.items(rows);
  }
  // Clicks the row's own "Mark as read" button.
  window.__markRead = function (id) {
    var conv = decodeURIComponent(id.split('/').pop());
    var rows = document.querySelectorAll('[role="listbox"] [role="option"][data-convid]');
    for (var i = 0; i < rows.length; i++) {
      if (rows[i].getAttribute('data-convid') !== conv) continue;
      var b = rows[i].querySelector('button[title="Mark as read"]');
      if (b) b.click();
      else window.__rt.log('mark read: no button');
    }
    setTimeout(loadMails, 1500);
  };
  window.__loadMails = loadMails;
  window.__rt.poll(loadMails);
}
