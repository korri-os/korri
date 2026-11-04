// Eight cards; left and right move the focus, the band recentres on it.
const CARDS = 8;
globalThis.surface = {
  init: () => ({ focus: 3 }),
  update: (s, m) => {
    if (m._tag !== "Pressed") return { state: s, requests: [] };
    if (m.button === "right" && s.focus < CARDS - 1) return { state: { focus: s.focus + 1 }, requests: [] };
    if (m.button === "left" && s.focus > 0) return { state: { focus: s.focus - 1 }, requests: [] };
    return { state: s, requests: [] };
  },
  view: (s) => ({
    current: s.focus,
    cards: Array.from({ length: CARDS }, (_, i) => ({ label: "Card", focused: i === s.focus, focusRegion: "band" })),
  }),
};
