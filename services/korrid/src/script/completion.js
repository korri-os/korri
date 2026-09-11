// The private array is also retained by Rust so cleanup can release callback
// references through its native length property, without executing more JS.
((now, queue) => {
  const global = globalThis;
  const apply = Reflect.apply;
  const finite = Number.isFinite;
  const trunc = Math.trunc;
  const maximum = Math.max;
  const maximumId = Number.MAX_SAFE_INTEGER;
  const TypeErrorConstructor = TypeError;
  const RangeErrorConstructor = RangeError;
  Object.setPrototypeOf(queue, null);
  let nextId = 1;

  global.setTimeout = (callback, delay = 0, ...args) => {
    if (typeof callback !== "function") {
      throw new TypeErrorConstructor("Plugin timers require a function");
    }
    // Coercion can execute plugin code, so allocate the ID only afterward.
    delay = +delay;
    if (!finite(delay) || delay < 0) delay = 0;
    if (delay > 2147483647) delay = 1;
    delay = trunc(delay);
    if (nextId > maximumId) throw new RangeErrorConstructor("Timer IDs exhausted");
    const id = nextId++;
    let slot = 0;
    while (slot < queue.length && queue[slot] !== undefined) slot++;
    queue[slot] = { id, callback, args, due: now() + delay, slot };
    return id;
  };
  global.clearTimeout = value => {
    const id = +value;
    for (let slot = 0; slot < queue.length; slot++) {
      const timer = queue[slot];
      if (timer !== undefined && timer.id === id) {
        queue[slot] = undefined;
        return;
      }
    }
  };

  function earliest() {
    let selected;
    for (let slot = 0; slot < queue.length; slot++) {
      const timer = queue[slot];
      if (timer === undefined) continue;
      if (selected === undefined || timer.due < selected.due ||
          (timer.due === selected.due && timer.id < selected.id)) selected = timer;
    }
    return selected;
  }
  return {
    next() {
      const timer = earliest();
      return timer === undefined ? -1 : maximum(0, timer.due - now());
    },
    tick() {
      const timer = earliest();
      if (timer === undefined || timer.due > now()) return;
      queue[timer.slot] = undefined;
      apply(timer.callback, global, timer.args);
    },
  };
})
