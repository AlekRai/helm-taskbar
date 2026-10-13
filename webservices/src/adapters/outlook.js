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

  // Today's timed events from the top bar's "upcoming events" button (Outlook keeps it current), as title, start, end (unix seconds).
  var LABEL = /^(.*?), (\d{1,2}:\d{2}(?: ?[AP]M)?) to (\d{1,2}:\d{2}(?: ?[AP]M)?), [A-Za-z]+, ([A-Za-z]+ \d{1,2}, \d{4})(.*)$/;
  var lastEvents = null;
  var evs = {};
  function readEvents() {
    var btn = Array.from(document.querySelectorAll('button[aria-label]')).filter(function (b) { return /^\d+ upcoming events?\b/.test(b.getAttribute('aria-label')); })[0];
    // No button once the loaded page has no events left today.
    if (!btn && !document.querySelector('[role="treeitem"][data-folder-name]')) return;
    var today = new Date().toDateString(), out = [];
    (btn ? btn.getAttribute('aria-label') : '').replace(/^\d+ upcoming events?\s*/, '').split(/\s*\b\d+ of \d+ /).forEach(function (part) {
      var m = LABEL.exec(part);
      if (!m || /^(Canceled|Cancelled|Declined):/i.test(m[1]) || /\b(Declined|Canceled|Cancelled|Following|Follow)\b/.test(m[5])) return;
      var start = new Date(m[4] + ' ' + m[2]), end = new Date(m[4] + ' ' + m[3]);
      if (isNaN(start) || isNaN(end) || start.toDateString() !== today) return;
      var status = /\bAccepted\b/.test(m[5]) ? 'accepted' : /\bTentative\b/.test(m[5]) ? 'tentative' : 'none';
      var title = m[1].replace(/[\t\r\n|]+/g, ' ').trim();
      evs[Math.floor(start / 1000) + '\t' + title] = { title: m[1], time: m[2] };
      out.push([title, Math.floor(start / 1000), Math.floor(end / 1000), status].join('\t'));
    });
    var k = out.join('\n');
    if (k !== lastEvents) { lastEvents = k; window.chrome.webview.postMessage('events|' + k); }
  }
  // Drives the meeting chip: opens the event in the top bar's upcoming-events list, then joins it
  // (posting its Teams link) or sets the response (accepted, tentative, declined).
  function until(test, done, fail) {
    var n = 0, t = setInterval(function () {
      var r = test();
      if (r) { clearInterval(t); done(r); } else if (++n > 70) { clearInterval(t); if (fail) fail(); }
    }, 150);
  }
  function visible(sel, re) {
    return Array.from(document.querySelectorAll(sel)).filter(function (b) {
      return b.offsetParent && (!re || re.test(b.getAttribute('aria-label') || b.textContent));
    })[0];
  }
  window.__meeting = function (action, start, title) {
    var ev = evs[start + '\t' + title];
    if (!ev) return window.__rt.log('meeting: unknown event');
    var card = function () {
      return Array.from(document.querySelectorAll('div[tabindex="0"][aria-label]')).filter(function (d) {
        return d.offsetParent && d.getAttribute('aria-label').indexOf(ev.title + ', ' + ev.time + ' to ') >= 0;
      })[0];
    };
    if (!card()) {
      var btn = Array.from(document.querySelectorAll('button[aria-label]')).filter(function (b) { return /^\d+ upcoming events?\b/.test(b.getAttribute('aria-label')); })[0];
      if (btn) btn.click();
    }
    var close = function () { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); };
    until(card, function (c) {
      c.click();
      if (action === 'join') {
        until(function () { return visible('button', /^\W*Join\W*$/); }, function (j) {
          var urls = [], open = window.open;
          window.open = function (u) { urls.push(String(u)); return null; };
          j.click();
          setTimeout(function () {
            window.open = open;
            if (urls[0]) window.chrome.webview.postMessage('join|' + urls[0]);
            else window.__rt.log('meeting: join gave no link');
            close();
          }, 1200);
        }, function () { window.__rt.log('meeting: no join button'); close(); });
        return;
      }
      var finish = function () {
        // Recurring events ask which occurrences to change; some responses ask to send it.
        until(function () { return visible('[role="menuitem"]', /^\W*This event\W*$/); }, function (m) { m.click(); }, null);
        setTimeout(function () {
          var s = visible('button', /^\W*Send\W*$/);
          if (s) s.click();
          setTimeout(function () { close(); readEvents(); }, 1500);
        }, 2500);
      };
      if (action === 'tentative') {
        until(function () { return Array.from(document.querySelectorAll('button[aria-label="More options"]')).filter(function (b) { return b.offsetParent && b.getBoundingClientRect().top > 300; })[0]; }, function (more) {
          more.click();
          until(function () { return visible('[role="menuitem"]', /^\W*Tentative\W*$/); }, function (m) { m.click(); finish(); }, close);
        }, close);
      } else {
        var label = { accepted: 'Accept', follow: 'Follow' }[action] || 'Decline';
        until(function () { return visible('button[aria-label^="' + label + '"]', null); }, function (b) { b.click(); finish(); }, close);
      }
    }, function () { window.__rt.log('meeting: event not in list'); close(); });
  };
  setInterval(readEvents, 15000);
  setTimeout(readEvents, 8000);

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
