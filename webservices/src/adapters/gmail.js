// Unread count = the Inbox entry in the sidebar ("Inbox 5 unread"), which matches the Gmail tab title.
// The unread Primary mails (sender, subject, link) come from the Primary-tab atom feed.
if (window.top === window) {
  var base = (/^\/mail\/u\/\d+\//.exec(location.pathname) || ['/mail/u/0/'])[0];
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
    fetch(base + 'feed/atom/%5Esq_ig_i_personal', { credentials: 'include' })
      .then(function (r) { return r.text(); })
      .then(function (t) {
        var doc = new DOMParser().parseFromString(t, 'text/xml');
        var rows = Array.from(doc.getElementsByTagName('entry')).map(function (e) {
          var href = e.getElementsByTagName('link')[0].getAttribute('href');
          var id = /message_id=([0-9a-f]+)/.exec(href);
          var from = e.getElementsByTagName('name')[0];
          return {
            from: from ? from.textContent : '',
            title: e.getElementsByTagName('title')[0].textContent,
            url: id ? 'https://mail.google.com' + base + '#inbox/' + id[1] : '',
          };
        });
        window.__rt.items(rows);
      })
      .catch(function () {});
  }
  setInterval(loadMails, 10000);
  setTimeout(loadMails, 4000);
}
