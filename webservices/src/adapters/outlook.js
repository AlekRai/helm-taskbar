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
}
