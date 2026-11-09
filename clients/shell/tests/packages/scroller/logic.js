// Ten rows, four in view: up and down move the focus and scroll to keep it in view.
const ROWS = 10, SHOWN = 4;
globalThis.surface = {
  init: () => ({ focus: 0, top: 0 }),
  update: (s, m) => {
    if (m._tag !== "Pressed") return { state: s, requests: [] };
    let focus = s.focus;
    if (m.button === "down" && focus < ROWS - 1) focus += 1;
    else if (m.button === "up" && focus > 0) focus -= 1;
    else return { state: s, requests: [] };
    const top = Math.min(Math.max(s.top, focus - SHOWN + 1), focus);
    return { state: { focus, top }, requests: [] };
  },
  view: (s) => ({
    top: s.top,
    rows: Array.from({ length: ROWS }, (_, i) => ({ label: "Same", focused: i === s.focus, focusId: "list:" + i, focusRegion: "list" })),
  }),
};
