// Tabs above a grid of cells. Up from the grid's top row enters the tabs at
// the active tab; down from the tabs enters the grid where it last was.
// WRONG=1 in the model's clockLabel makes left in the grid jump to tab 0.
const TABS = ["Games", "Options", "Data"];
const CELLS = ["A", "B", "C", "D", "E", "F"];
const COLUMNS = 3;
globalThis.surface = {
  init: () => ({ region: "cells", tab: 1, active: 1, cell: 0 }),
  update: (s, m, model) => {
    if (m._tag !== "Pressed") return { state: s, requests: [] };
    const b = m.button;
    if (s.region === "tabs") {
      if (b === "left" && s.tab > 0) return { state: { ...s, tab: s.tab - 1 }, requests: [] };
      if (b === "right" && s.tab < TABS.length - 1) return { state: { ...s, tab: s.tab + 1 }, requests: [] };
      if (b === "confirm") return { state: { ...s, active: s.tab }, requests: [] };
      if (b === "down") return { state: { ...s, region: "cells" }, requests: [] };
      return { state: s, requests: [] };
    }
    const row = Math.floor(s.cell / COLUMNS), col = s.cell % COLUMNS;
    if (b === "left" && col > 0) return { state: { ...s, cell: model.clockLabel === "WRONG" ? 0 : s.cell - 1 }, requests: [] };
    if (b === "right" && col < COLUMNS - 1) return { state: { ...s, cell: s.cell + 1 }, requests: [] };
    if (b === "down" && row < CELLS.length / COLUMNS - 1) return { state: { ...s, cell: s.cell + COLUMNS }, requests: [] };
    if (b === "up" && row > 0) return { state: { ...s, cell: s.cell - COLUMNS }, requests: [] };
    if (b === "up") return { state: { ...s, region: "tabs", tab: s.active }, requests: [] };
    return { state: s, requests: [] };
  },
  view: (s) => ({
    tabs: TABS.map((label, i) => ({ label, focused: s.region === "tabs" && s.tab === i, focusId: "tabs:" + i, focusRegion: "tabs", focusActive: i === s.active })),
    cells: CELLS.map((label, i) => ({ label, focused: s.region === "cells" && s.cell === i, focusId: "cells:" + i, focusRegion: "cells" })),
    focusPlan: {
      tabs: { down: { to: "cells", entry: "remembered" } },
      cells: { up: { to: "tabs", entry: "active" } },
    },
  }),
};
