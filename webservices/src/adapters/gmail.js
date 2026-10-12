if (window.top === window) {
  // Atom feed of the unread inbox, fetched with the page's own cookies.
  var base = (/^\/mail\/u\/\d+\//.exec(location.pathname) || ['/mail/u/0/'])[0];
  setInterval(function () {
    if (location.hostname !== 'mail.google.com') return;
    fetch(base + 'feed/atom', { credentials: 'include' })
      .then(function (r) { return r.text(); })
      .then(function (t) {
        var m = /<fullcount>(\d+)<\/fullcount>/.exec(t);
        if (m) window.__rt.report(parseInt(m[1], 10), false);
      })
      .catch(function () {});
  }, 5000);
}
