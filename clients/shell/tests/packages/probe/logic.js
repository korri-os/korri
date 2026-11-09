// A test package for the shell's integration tests. Plain JavaScript, no bundle.
globalThis.surface = {
  preferences: { "probe.word": { mark: "device", initial: "first" } },
  init: (model, preferences) => ({ word: preferences["probe.word"], count: 0, stored: false, idle: -1, ticks: 0 }),
  update: (state, message, model) => {
    if (message._tag === "Dragged") return { state: { ...state, drag: JSON.stringify(message) }, requests: [] };
    if (message._tag === "Activated") return { state: { ...state, activation: JSON.stringify(message) }, requests: [] };
    if (message._tag === "PreferenceStored") return { state: { ...state, stored: message.key === "probe.word" }, requests: [] };
    // A2: the first Resized must come before the first view (views counts them).
    if (message._tag === "Resized") return { state: { ...state, width: message.width, height: message.height, resizes: (state.resizes || 0) + 1, viewsBeforeFirstResize: state.resizes ? state.viewsBeforeFirstResize : (globalThis.views || 0) }, requests: [] };
    // A model with clockLabel "crash" makes the logic throw (a crash, amendment A5).
    if (message._tag === "KorriPublished" && model.clockLabel === "crash") throw new Error("planted crash");
    if (message._tag === "Tick") return { state: { ...state, idle: message.idleMs, ticks: state.ticks + 1 }, requests: [] };
    if (message._tag !== "Pressed") return { state, requests: [] };
    state = { ...state, presses: (state.presses || 0) + 1 };
    switch (message.button) {
      case "right": return { state: { ...state, count: state.count + 1 }, requests: [] };
      case "left": return { state: { ...state, count: state.count - 1 }, requests: [] };
      case "options": return { state: { ...state, word: "second" }, requests: [{ _tag: "RememberPreference", key: "probe.word", value: "second" }] };
      // Slow, not stuck: 120 ms, over the 50 ms budget (amendment A5).
      case "up": { const t0 = new Date().getTime(); while (new Date().getTime() - t0 < 120) {} return { state: { ...state, count: state.count + 10 }, requests: [] }; }
      case "down": return { state, requests: [{ _tag: "MoveCatalog", query: { text: "", section: null, order: "korri" }, from: { _tag: "End" }, count: 50 }] };
      case "back": return { state, requests: [{ _tag: "DismissGameplayOverlay" }] };
      case "confirm": while (true) {}
      case "menu": { const a = []; while (true) a.push(new Uint8Array(1 << 20)); }
      case "system": return { state, requests: [{ _tag: "Teleport" }] };
      default: return { state, requests: [] };
    }
  },
  view: (state, model) => (globalThis.views = (globalThis.views || 0) + 1, {
    activation: state.activation || "",
    drag: state.drag || "",
    widthSeen: state.width || 0,
    heightSeen: state.height || 0,
    resizes: state.resizes || 0,
    viewsBeforeFirstResize: state.viewsBeforeFirstResize ?? -1,
    idleMs: state.idle,
    ticks: state.ticks,
    word: state.word,
    count: state.count,
    presses: state.presses || 0,
    total: model.catalog._tag === "Ready" ? model.catalog.total : 0,
    stored: state.stored,
    focusedGame: model.focusedGame ? model.focusedGame.gameId : "",
  }),
  overlay: (state, model) => ({
    activation: state.activation || "",
    drag: state.drag || "",
    heading: model.presentation.kind === "gameplay-overlay" ? (model.presentation.title || "") : "",
    controls: model.presentation.kind === "gameplay-overlay" ? model.presentation.controls.length : 0,
  }),
};
