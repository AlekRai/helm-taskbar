// Unread count = the Inbox entry in the sidebar ("Inbox 5 unread"), which matches the Gmail tab title.
// The unread Primary mails (sender, subject, link) come from the Primary-tab atom feed.
if (window.top === window) {
  var gmBase = (/^\/mail\/u\/\d+\//.exec(location.pathname) || ['/mail/u/0/'])[0];
  setInterval(function () {
    // In low-memory mode the hidden page is the atom feed; a sign-in page there means the full page is needed.
    if (location.hostname === 'accounts.google.com') window.chrome.webview.postMessage('login|');
    if (location.hostname !== 'mail.google.com') return;
    // The tab title carries the account: "Inbox (5) - name@gmail.com - Gmail".
    var who = /([\w.+-]+)@[\w.-]+/.exec(document.title);
    if (who) window.__rt.name(who[1]);
    if (feedOk) return;
    var link = document.querySelector('a.J-Ke[aria-label^="Inbox"]');
    if (!link) return;
    var m = /(\d[\d,]*)\s+unread/i.exec(link.getAttribute('aria-label'));
    window.__rt.report(m ? parseInt(m[1].replace(/,/g, ''), 10) : 0, false);
  }, 2000);

  // Once the Primary feed answers it is the count source (the sidebar can miss unread mail).
  var feedOk = false;
  function loadMails() {
    if (location.hostname !== 'mail.google.com') return;
    fetch(gmBase + 'feed/atom/%5Esq_ig_i_personal', { credentials: 'include' })
      .then(function (r) { return r.text(); })
      .then(function (t) {
        var dec = function (s) {
          return s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;|&apos;/g, "'").replace(/&amp;/g, '&');
        };
        var rows = (t.match(/<entry>[\s\S]*?<\/entry>/g) || []).map(function (e) {
          var id = /message_id=([0-9a-f]+)/.exec(e);
          var title = /<title>([\s\S]*?)<\/title>/.exec(e);
          var sum = /<summary>([\s\S]*?)<\/summary>/.exec(e);
          var from = /<name>([\s\S]*?)<\/name>/.exec(e);
          return {
            from: from ? dec(from[1]) : '',
            title: title ? dec(title[1]) : '',
            preview: sum ? dec(sum[1]) : '',
            url: id ? 'https://mail.google.com' + gmBase + '#inbox/' + id[1] : '',
          };
        });
        var who = /for ([\w.+-]+)@/.exec(t.slice(0, 400));
        if (who) window.__rt.name(who[1]);
        var full = /<fullcount>(\d+)<\/fullcount>/.exec(t);
        if (full || rows.length) {
          feedOk = true;
          window.__rt.report(Math.max(full ? parseInt(full[1], 10) : 0, rows.length), false);
        }
        window.__rt.items(rows);
      })
      .catch(function (e) { window.__rt.log('mails error: ' + e); });
  }
  // Marks a conversation read through Gmail's own action endpoint, then re-reads the unread list.
  // The action token is a session cookie that only the full Gmail page sets, so the light feed page
  // loads the full page once, finishes the job there and returns to the feed.
  function sendRead(id) {
    var at = (document.cookie.match(/GMAIL_AT=([^;]+)/) || [])[1];
    if (!at) return Promise.resolve(false);
    return getIk().then(function (ik) {
      if (!ik) return false;
      return fetch(gmBase + '?ui=2&ik=' + ik + '&at=' + at + '&view=up&act=rd&pcd=1&mb=0&rt=j&search=inbox', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: 't=' + id,
      }).then(function (r) { window.__rt.log('mark read: ' + r.status); return true; });
    });
  }
  window.__markRead = function (id) {
    id = id.split('/').pop();
    if (!/^[0-9a-f]+$/.test(id)) return;
    sendRead(id).then(function (done) {
      if (done) return loadMails();
      sessionStorage.setItem('__pendingRead', id);
      location.href = gmBase;
    }).catch(function (e) { window.__rt.log('mark read error: ' + e); });
  };
  var waits = 0;
  setInterval(function () {
    var id = location.hostname === 'mail.google.com' && window.GLOBALS && sessionStorage.getItem('__pendingRead');
    if (!id) return;
    // The cookie appears a moment after the page object does.
    if (!/GMAIL_AT=/.test(document.cookie) && ++waits < 20) return;
    sessionStorage.removeItem('__pendingRead');
    sendRead(id).then(function (done) {
      if (!done) window.__rt.log('mark read: no action token');
      loadMails();
      if (!window.__shown) setTimeout(function () { location.href = gmBase + 'feed/atom'; }, 1500);
    });
  }, 1000);  // Mailbox key: from the full page, or (on the feed page) from the page source of the mailbox.
  var ikCache = null;
  function getIk() {
    if (window.GLOBALS) return Promise.resolve(String(window.GLOBALS[9]));
    if (ikCache) return Promise.resolve(ikCache);
    return fetch(gmBase, { credentials: 'include' }).then(function (r) { return r.text(); }).then(function (t) {
      var m = /GLOBALS=\[null,null,\d+,"[^"]*","[^"]*","[^"]*","[^"]*","[^"]*",\d+,"([0-9a-f]+)"/.exec(t);
      return (ikCache = m ? m[1] : null);
    });
  }
  window.__loadMails = loadMails;
  window.__rt.poll(loadMails, 10000);
  setTimeout(loadMails, 4000);
}
