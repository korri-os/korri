// Runs after korrid has compiled the whole graph and before any plugin code.
// It removes every route by which plugin code could compile source at run
// time: eval, Function, and the async, generator and async generator Function
// constructors. A plugin can no longer make QuickJS parse code after it has
// filled the heap. Function.prototype is unchanged, so `instanceof Function`
// and every existing function keep working.
(() => {
  "use strict";
  const original = Function;
  const prototypes = [
    original.prototype,
    Object.getPrototypeOf(async function () {}),
    Object.getPrototypeOf(function* () {}),
    Object.getPrototypeOf(async function* () {}),
  ];
  const refuse = function Function() {
    throw new EvalError("plugin code generation is not permitted");
  };
  Object.defineProperty(refuse, "prototype", {
    value: original.prototype,
    writable: false,
    enumerable: false,
    configurable: false,
  });
  for (const prototype of prototypes) {
    Object.defineProperty(prototype, "constructor", {
      value: refuse,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }
  Object.defineProperty(globalThis, "Function", {
    value: refuse,
    writable: true,
    enumerable: false,
    configurable: true,
  });
  delete globalThis.eval;
})();
