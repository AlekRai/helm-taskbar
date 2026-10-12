// Unread count = the Inbox entry in the sidebar ("Inbox 5 unread"), which matches the Gmail tab title.
// The unread Primary mails (sender, subject, link) come from the Primary-tab atom feed.
if (window.top === window) {
  var gmBase = (/^\/mail\/u\/\d+\//.exec(location.pathname) || ['/mail/u/0/'])[0];
  setInterval(function () {
    if (location.hostname !== 'mail.google.com') return;
    // The tab title carries the account: "Inbox (5) - name@gmail.com - Gmail".
    var who = /([\w.+-]+)@[\w.-]+/.exec(document.title);
    if (who) window.__rt.name(who[1]);
    var link = document.querySelector('a.J-Ke[aria-label^="Inbox"]');
    if (!link) return;
    var m = /(\d[\d,]*)\s+unread/i.exec(link.getAttribute('aria-label'));
    window.__rt.report(m ? parseInt(m[1].replace(/,/g, ''), 10) : 0, false);
  }, 2000);

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
          var from = /<name>([\s\S]*?)<\/name>/.exec(e);
          return {
            from: from ? dec(from[1]) : '',
            title: title ? dec(title[1]) : '',
            url: id ? 'https://mail.google.com' + gmBase + '#inbox/' + id[1] : '',
          };
        });
        window.__rt.log('mails: ' + rows.length + ' ' + t.length);
        window.__rt.items(rows);
      })
      .catch(function (e) { window.__rt.log('mails error: ' + e); });
  }
  window.__loadMails = loadMails;
  setInterval(loadMails, 10000);
  setTimeout(loadMails, 4000);
}
