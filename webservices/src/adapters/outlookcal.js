// Hidden Outlook day view: reports today's timed events as (title, start, end) in unix seconds.
if (window.top === window) {
  var loadedDay = new Date().toDateString();
  var lastEvents = null;
  var LABEL = /^(.*?), (\d{1,2}:\d{2}(?: ?[AP]M)?) to (\d{1,2}:\d{2}(?: ?[AP]M)?), [A-Za-z]+, ([A-Za-z]+ \d{1,2}, \d{4})(.*)$/;

  function readEvents() {
    var seen = {}, out = [], today = new Date().toDateString();
    document.querySelectorAll('[aria-label]').forEach(function (e) {
      var m = LABEL.exec(e.getAttribute('aria-label'));
      if (!m || /^(Canceled|Cancelled|Declined):/i.test(m[1]) || /\b(Declined|Canceled|Cancelled)\b/.test(m[5])) return;
      var start = new Date(m[4] + ' ' + m[2]), end = new Date(m[4] + ' ' + m[3]);
      if (isNaN(start) || isNaN(end) || start.toDateString() !== today) return;
      var title = m[1].replace(/[\t\r\n|]+/g, ' ').trim();
      var key = title + start.getTime();
      if (seen[key]) return;
      seen[key] = 1;
      out.push([title, Math.floor(start.getTime() / 1000), Math.floor(end.getTime() / 1000)].join('\t'));
    });
    return out.join('\n');
  }

  setInterval(function () {
    // The page keeps showing the day it was opened on.
    if (new Date().toDateString() !== loadedDay) { location.reload(); return; }
    if (!/^Calendar/.test(document.title)) return;
    var k = readEvents();
    if (k !== lastEvents) { lastEvents = k; window.chrome.webview.postMessage('events|' + k); }
  }, 10000);
}
