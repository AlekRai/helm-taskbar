// Unread count = the Inbox entry in the sidebar ("Inbox 5 unread"), which matches the Gmail tab title.
if (window.top === window) {
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
}
