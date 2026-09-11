// Executes against the same installed globals as policy evaluations.
globalThis.timerResults = { events: [], checks: {} };
{
  const { events, checks } = timerResults;
  let inline = true;
  const argument = { marker: "identity" };
  const before = Date.now();
  const cancelled = setTimeout(() => events.push("cancelled"), 0);
  clearTimeout(String(cancelled));
  clearTimeout(cancelled);
  clearTimeout(999999);
  checks.invalidCallback = false;
  try { setTimeout("not evaluated", 0); } catch (e) { checks.invalidCallback = e instanceof TypeError; }
  const ids = Array.from({ length: 256 }, () => setTimeout(() => {}, 1000));
  checks.retentionBound = false;
  try { setTimeout(() => {}, 1000); } catch (e) { checks.retentionBound = e instanceof RangeError; }
  ids.forEach(clearTimeout);
  let later;
  setTimeout(function (a, b) {
    events.push("zero");
    checks.nonInline = !inline;
    checks.arguments = a === argument && b === 42;
    checks.receiver = this === globalThis;
    clearTimeout(later);
    Promise.resolve().then(() => events.push("microtask"));
    setTimeout(() => events.push("nested"), 0);
  }, 0, argument, 42);
  later = setTimeout(() => events.push("cancelled-by-callback"), 0);
  setTimeout(() => {
    events.push("delayed");
    checks.realDelay = Date.now() - before >= 12;
  }, 12);
  setTimeout(() => events.push("negative"), -1);
  inline = false;
}
