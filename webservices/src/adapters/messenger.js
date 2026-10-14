// Unread count = the "Chats · N unread" label of the chat list, or the rows marked "Unread message".
// Messenger sometimes keeps a chat unread when its last event is a reaction; those older than a day are ignored.
function staleReaction(a) {
  var lines = a.innerText.split('\n').map(function (s) { return s.trim(); });
  var at = lines.findIndex(function (s) { return /^Unread message/i.test(s); });
  if (at < 0 || !/^Reacted\b.*\bto your message/i.test(lines.slice(at + 1).join(' '))) return false;
  var limit = window.__rt.opt('stale', 24);
  if (!limit) return false;
  var age = lines.filter(function (s) { return /^\d+\s*[smhdwy]$/.test(s); })[0] || '';
  var m = /^(\d+)\s*([smhdwy])$/.exec(age);
  if (!m) return false;
  var hours = { s: 0, m: 0, h: 1, d: 24, w: 168, y: 8760 }[m[2]] * parseInt(m[1], 10);
  return hours >= limit;
}

// Muted chats carry a crossed-bell icon (12x13 viewBox) in their row.
function muted(a) {
  return !!a.querySelector('svg[viewBox="0 0 12 13"]');
}

if (window.top === window) {
  setInterval(function () {
    if (!document.querySelector('[role="grid"], [aria-label^="Chats"]')) return;
    var tab = document.querySelector('a[aria-label^="Chats"][aria-label*="unread"]');
    var m = tab && /(\d+)\s+unread/i.exec(tab.getAttribute('aria-label'));
    if (m) {
      var stale = Array.from(document.querySelectorAll('[role="grid"] a')).filter(function (a) { return staleReaction(a) || (muted(a) && /Unread message/i.test(a.innerText) && !staleReaction(a)); }).length;
      window.__rt.report(Math.max(0, parseInt(m[1], 10) - stale), false);
      return;
    }
    var rows = Array.from(document.querySelectorAll('[role="grid"] a')).filter(function (a) {
      return /Unread message/i.test(a.innerText) && !staleReaction(a) && !muted(a);
    });
    window.__rt.report(rows.length, false);
  }, 2000);

  // Unread conversations: name, last message and a link that opens the chat.
  function loadMails() {
    var links = document.querySelectorAll('[role="grid"] a[href*="/t/"]');
    if (!links.length) return;
    var rows = [];
    links.forEach(function (a) {
      var t = a.innerText;
      if (!/Unread message/i.test(t) || staleReaction(a) || muted(a)) return;
      var lines = t.split('\n').map(function (s) { return s.trim(); }).filter(function (s) {
        return s && s !== '\u00b7' && s !== 'Active now' && s !== 'Reply?' && !/^\d+\s*[smhdwy]$/.test(s);
      });
      var at = lines.findIndex(function (s) { return /^Unread message/i.test(s); });
      rows.push({
        from: lines[0] || '',
        title: '',
        preview: at >= 0 ? lines.slice(at + 1).join(' ') : '',
        url: location.origin + a.getAttribute('href'),
      });
    });
    window.__rt.items(rows);
  }
  // Marks a chat read through the row's "More options" menu.
  window.__markRead = function (id) {
    var a = document.querySelector('[role="grid"] a[href="' + new URL(id, location.origin).pathname + '"]') ||
      Array.from(document.querySelectorAll('[role="grid"] a[href*="/t/"]')).find(function (x) { return id.indexOf(x.getAttribute('href')) >= 0; });
    var row = a;
    for (var i = 0; i < 8 && row && !row.querySelector('[aria-label^="More options"]'); i++) row = row.parentElement;
    var more = row && row.querySelector('[aria-label^="More options"]');
    if (!more) { window.__rt.log('mark read: no menu'); return; }
    more.click();
    setTimeout(function () {
      var item = Array.from(document.querySelectorAll('[role="menuitem"]')).find(function (x) { return /^Mark as read/i.test(x.innerText); });
      if (item) item.click(); else window.__rt.log('mark read: no item');
      setTimeout(loadMails, 1200);
    }, 500);
  };
  window.__loadMails = loadMails;
  window.__rt.poll(loadMails);
}
