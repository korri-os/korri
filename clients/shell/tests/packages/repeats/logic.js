// Two route rows with the same buttons ("Play", "Back up"), a header that
// says "Back up" too, and a choice row in a region of its own: left and
// right step its value in place, so focus stays.
const LABELS = ["Play", "Back up"];
const VALUES = ["Low", "Mid", "High"];
globalThis.surface = {
  init: () => ({ row: 0, col: 0, value: 0 }),
  update: (s, m) => {
    if (m._tag !== "Pressed") return { state: s, requests: [] };
    const b = m.button;
    if (s.row === 2) {
      if (b === "left" && s.value > 0) return { state: { ...s, value: s.value - 1 }, requests: [] };
      if (b === "right" && s.value < VALUES.length - 1) return { state: { ...s, value: s.value + 1 }, requests: [] };
      if (b === "up") return { state: { ...s, row: 1 }, requests: [] };
      return { state: s, requests: [] };
    }
    if (b === "left" && s.col > 0) return { state: { ...s, col: s.col - 1 }, requests: [] };
    if (b === "right" && s.col < LABELS.length - 1) return { state: { ...s, col: s.col + 1 }, requests: [] };
    if (b === "down") return { state: { ...s, row: s.row + 1 }, requests: [] };
    if (b === "up" && s.row > 0) return { state: { ...s, row: s.row - 1 }, requests: [] };
    return { state: s, requests: [] };
  },
  view: (s) => ({
    header: "Back up",
    first: LABELS.map((label, i) => ({ label, focused: s.row === 0 && s.col === i, focusRegion: "rows" })),
    second: LABELS.map((label, i) => ({ label, focused: s.row === 1 && s.col === i, focusRegion: "rows" })),
    choice: { label: "Quality", value: VALUES[s.value], focused: s.row === 2, focusRegion: "choice" },
    focusPlan: {
      rows: { down: { to: "choice", entry: "first" } },
      choice: { up: { to: "rows", entry: "remembered" } },
    },
  }),
};
