// Unshipped experiment. The only host input is monotonic elapsed milliseconds.
// JS owns every queued reference; Rust holds only this controller in its Ctx.
// 256 retained callbacks is an experimental probe bound, NOT product policy.
(now => {
  const limit = 256;
  const queue = new Map();
  // Cleanup cannot dispatch to a prototype method replaced by evaluated source.
  const clearQueue = queue.clear.bind(queue);
  const queueSize = Object.getOwnPropertyDescriptor(Map.prototype, "size").get.bind(queue);
  let nextId = 1, scheduled = 0, fired = 0, cancelled = 0, highWater = 0;
  let closed = false;
  globalThis.setTimeout = (callback, delay = 0, ...args) => {
    if (closed) throw new Error("Timer evaluation closed");
    if (typeof callback !== "function") throw new TypeError("Probe timers require a function");
    // Conversion may execute JS. Check retention only after conversion finishes.
    delay = +delay;
    if (!Number.isFinite(delay) || delay < 0) delay = 0;
    if (delay > 2147483647) delay = 1;
    delay = Math.trunc(delay);
    if (queue.size >= limit) throw new RangeError("Probe timer queue limit: 256");
    if (nextId > Number.MAX_SAFE_INTEGER) throw new RangeError("Probe timer IDs exhausted");
    const id = nextId++;
    queue.set(id, { callback, args, due: now() + delay });
    scheduled++;
    highWater = Math.max(highWater, queue.size);
    return id;
  };
  globalThis.clearTimeout = id => {
    if (queue.delete(+id)) cancelled++;
  };
  function earliest() {
    let selected;
    for (const [id, timer] of queue) {
      if (selected === undefined || timer.due < selected[1].due) selected = [id, timer];
    }
    return selected;
  }
  return {
    next() {
      const selected = earliest();
      return selected === undefined ? -1 : Math.max(0, selected[1].due - now());
    },
    tick() {
      const selected = earliest();
      if (selected === undefined || selected[1].due > now()) return;
      const [id, timer] = selected;
      // Remove before invoking: cancellation/nesting cannot retain a fired entry.
      queue.delete(id);
      fired++;
      Reflect.apply(timer.callback, globalThis, timer.args);
    },
    stats() { return { scheduled, fired, cancelled, pending: queueSize(), highWater, limit }; },
    close() {
      const discarded = queueSize();
      closed = true;
      clearQueue();
      return discarded;
    },
  };
})
