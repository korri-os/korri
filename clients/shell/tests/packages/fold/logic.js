// A 3x6 grid showing two rows, scrolled to keep the focus in view; right from
// the grid's right column enters a side list, scrolled out of view until then,
// at its first row (scrolled in). Left from the list returns to the grid.
// HIDE=1 in the model's clockLabel hides the side list (a hidden layer): right
// then finds nothing to enter, and focus stays.
const COLUMNS = 3, ROWS = 6, SHOWN = 2, SIDE = 4;
const keep = (top, row) => Math.min(Math.max(top, row - SHOWN + 1), row);
globalThis.surface = {
  init: () => ({ region: "cells", cell: 0, top: 0, side: 0, sideTop: SIDE }),
  update: (s, m, model) => {
    if (m._tag !== "Pressed") return { state: s, requests: [] };
    const b = m.button, hide = model.clockLabel === "HIDE";
    const same = { state: s, requests: [] };
    if (s.region === "side") {
      if (b === "down" && s.side < SIDE - 1) return { state: { ...s, side: s.side + 1, sideTop: keep(s.sideTop, s.side + 1) }, requests: [] };
      if (b === "up" && s.side > 0) return { state: { ...s, side: s.side - 1, sideTop: keep(s.sideTop, s.side - 1) }, requests: [] };
      if (b === "left") return { state: { ...s, region: "cells" }, requests: [] };
      return same;
    }
    const row = Math.floor(s.cell / COLUMNS), col = s.cell % COLUMNS;
    const go = (cell) => ({ state: { ...s, cell, top: keep(s.top, Math.floor(cell / COLUMNS)) }, requests: [] });
    if (b === "left" && col > 0) return go(s.cell - 1);
    if (b === "right" && col < COLUMNS - 1) return go(s.cell + 1);
    if (b === "right" && !hide) return { state: { ...s, region: "side", side: 0, sideTop: 0 }, requests: [] };
    if (b === "down" && row < ROWS - 1) return go(s.cell + COLUMNS);
    if (b === "up" && row > 0) return go(s.cell - COLUMNS);
    return same;
  },
  view: (s, model) => ({
    top: s.top,
    sideTop: s.sideTop,
    hideSide: model.clockLabel === "HIDE",
    cells: Array.from({ length: COLUMNS * ROWS }, (_, i) => ({ label: String(i), focused: s.region === "cells" && s.cell === i, focusRegion: "cells" })),
    side: Array.from({ length: SIDE }, (_, i) => ({ label: "S" + i, focused: s.region === "side" && s.side === i, focusRegion: "side" })),
    focusPlan: {
      cells: { right: { to: "side", entry: "first" } },
      side: { left: { to: "cells", entry: "remembered" } },
    },
  }),
};
