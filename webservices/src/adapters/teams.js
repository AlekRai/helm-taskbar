// Unread count = the Activity and Chat badges in the app bar (summed). Meeting = own presence "In a call".
if (window.top === window && location.hostname === 'teams.microsoft.com') {
  location.replace(location.href.replace('teams.microsoft.com', 'teams.cloud.microsoft'));
}
if (window.top === window) {
  setInterval(function () {
    var bar = document.querySelector('[data-tid="app-bar-wrapper"]');
    if (!bar) return;
    var total = 0;
    bar.querySelectorAll('*').forEach(function (e) {
      if (e.children.length || !/^\d+\+?$/.test(e.textContent.trim())) return;
      var near = Array.from(e.parentElement.parentElement.children).concat(Array.from(e.parentElement.children));
      var rail = near.some(function (s) { return (window.__rt.opt('activity', true) ? /^(Activity|Chat) \(/ : /^Chat \(/).test(s.getAttribute('aria-label') || ''); });
      if (rail) total += parseInt(e.textContent, 10);
    });
    // Own presence flips to "In a call" while joined to a call or meeting.
    var me = document.querySelector('[data-tid="me-control-avatar-trigger"]');
    var meeting = !!me && /status In a call/i.test(me.getAttribute('aria-label') || '');
    window.__rt.report(total, meeting);
  }, 2000);

  // Unread chats of the chat list; the "url" is a pseudo link that the host turns into a click or a deep link.
  function chatRows() {
    return Array.from(document.querySelectorAll('[role="treeitem"][data-item-type="chat"], [role="treeitem"][data-item-type="slice"]'));
  }
  function convOf(e) {
    return (e.getAttribute('data-fui-tree-item-value') || '').split('|').pop();
  }
  function loadMails() {
    var chats = chatRows();
    if (!chats.length) return;
    var rows = [];
    chats.forEach(function (e) {
      // Quick views (followed threads, tag mentions) carry the Activity badge.
      if (e.getAttribute('data-item-type') === 'slice') {
        if (convOf(e) === 'slice-activities-replies') return;
        var n = (e.innerText.match(/\n(\d+)\s*(\n|$)/) || [])[1];
        if (n) rows.push({ from: e.innerText.split('\n')[0] + ' (' + n + ')', title: '', url: 'teams-chat:' + convOf(e) });
        return;
      }
      if (!/chat_list_unread_text/.test(e.getAttribute('aria-labelledby') || '')) return;
      var ids = e.getAttribute('aria-labelledby').trim().split(/\s+/);
      var tid = ids.filter(function (i) { return /^title-chat-list-item/.test(i); })[0];
      var el = tid && document.getElementById(tid);
      rows.push({ from: el ? el.innerText : e.innerText.split('\n')[0], title: '', url: 'teams-chat:' + convOf(e) });
    });
    (window.__rt.opt('threads', true) ? document.querySelectorAll('[data-testid="message-slice-card-threads"]') : []).forEach(function (r) {
      var ind = r.querySelector('[data-tid="message-slice-card-unread-indicator"]');
      var dot = ind && ind.querySelector('[data-message-slice-card-unread-indicator]');
      if (!ind || !((dot && dot.getAttribute('aria-label')) || ind.innerHTML.length > 170)) return;
      var l = r.innerText.split('\n');
      rows.push({ from: l[0] || '', title: l[2] || '', preview: l.slice(3, -1).join(' '), url: 'teams-thread:' + r.id.split('|').pop() });
    });
    window.__rt.items(rows);
  }
  function findChat(id) {
    var conv = id.replace(/^teams-chat:/, '');
    return chatRows().filter(function (e) { return convOf(e) === conv; })[0];
  }
  // Opens a chat inside the running Teams page (the host then shows the window).
  window.__openChat = function (id) {
    if (/^teams-thread:/.test(id)) {
      var card = document.getElementById('message-slice-card-threads-ThreadsSliceCardItem|' + id.split(':')[1]);
      if (card) card.click(); else window.__rt.log('open thread: not in list');
      return;
    }
    var e = findChat(id);
    if (e) e.click(); else window.__rt.log('open chat: not in list');
  };
  // Marks a chat read through its right-click menu.
  window.__markRead = function (id) {
    var e = findChat(id);
    if (!e) return;
    e.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 5, clientY: 5, button: 2 }));
    setTimeout(function () {
      var item = Array.from(document.querySelectorAll('[role="menuitem"]')).find(function (x) { return /mark as read/i.test(x.innerText); });
      if (item) item.click(); else window.__rt.log('mark read: no menu item');
      setTimeout(loadMails, 1500);
    }, 500);
  };
  // Holds the Available status (with a duration, so Teams stops using idle detection) while the host's window is open,
  // and resets it when the window closes. Done once per day, so a status changed by hand meanwhile is left alone.
  function waitFor(fn, ms) {
    return new Promise(function (res, rej) {
      var t0 = Date.now();
      var iv = setInterval(function () {
        var r = fn();
        if (r) { clearInterval(iv); res(r); } else if (Date.now() - t0 > (ms || 4000)) { clearInterval(iv); rej(new Error('timeout')); }
      }, 100);
    });
  }
  function byTid(t) { return function () { return document.querySelector('[data-tid="' + t + '"]'); }; }
  function openStatusMenu() {
    var me = document.querySelector('[data-tid="me-control-avatar-trigger"]');
    if (!me) return Promise.reject(new Error('no avatar'));
    me.click();
    return waitFor(byTid('set-presence-status-menu-item')).then(function (i) { i.click(); });
  }
  function closeMenus() {
    ['keydown', 'keyup'].forEach(function (t) {
      document.dispatchEvent(new KeyboardEvent(t, { key: 'Escape', code: 'Escape', keyCode: 27, bubbles: true }));
    });
  }
  function pick(combo, re) {
    if (re.test(combo.innerText)) return Promise.resolve();
    combo.click();
    return waitFor(function () {
      return Array.from(document.querySelectorAll('[role="option"]')).find(function (o) { return re.test(o.innerText); });
    }).then(function (o) { o.click(); });
  }
  function holdAvailable() {
    return openStatusMenu().then(function () {
      return waitFor(byTid('set-availability-duration'));
    }).then(function (d) {
      d.click();
      return waitFor(function () {
        var c = document.querySelectorAll('[data-tid="me-control-set-availability"] button[role="combobox"]');
        return c.length >= 2 && c;
      });
    }).then(function (c) {
      return pick(c[0], /^\s*available\s*$/i).then(function () { return pick(c[1], /^\s*today\s*$/i); });
    }).then(function () {
      return waitFor(byTid('me-control-set-availability-update-availability-button'));
    }).then(function (b) { b.click(); });
  }
  function resetStatus() {
    return openStatusMenu().then(function () {
      return waitFor(byTid('set-availability-reset-status'));
    }).then(function (r) { r.click(); });
  }
  var availBusy = false;
  // Clock out: resets the status now and keeps the hold from running again.
  window.__clockOut = function () {
    availBusy = true;
    resetStatus().then(function () { localStorage.removeItem('__availDay'); }).catch(function (e) { window.__rt.log('clock out: ' + e.message); closeMenus(); });
  };
  window.__availStep = function () {
    if (availBusy || !window.__rt.opt('avail', false) || !document.querySelector('[data-tid="me-control-avatar-trigger"]')) return;
    var now = window.__rt.opt('availNow', false);
    var d = new Date().toDateString();
    var held = localStorage.getItem('__availDay');
    var job = null;
    if (now && held !== d) job = holdAvailable().then(function () { localStorage.setItem('__availDay', d); });
    else if (!now && held) job = resetStatus().then(function () { localStorage.removeItem('__availDay'); });
    if (!job) return;
    availBusy = true;
    job.catch(function (e) { window.__rt.log('available: ' + e.message); closeMenus(); }).then(function () {
      setTimeout(function () { availBusy = false; }, 30000);
    });
  };
  setInterval(window.__availStep, 10000);
  window.__loadMails = loadMails;
  window.__rt.poll(loadMails);
  // While the window is hidden the page rests on Followed threads, so its unread threads stay listed.
  setInterval(function () {
    if (window.__shown || !window.__rt.opt('threads', true) || !chatRows().length) return;
    var s = document.querySelector('[data-testid="slice-list-item-slice-activities-replies"]');
    if (s && !document.querySelector('[data-testid="message-slice-list-vl2-viewport"]')) s.click();
  }, 4000);
}
