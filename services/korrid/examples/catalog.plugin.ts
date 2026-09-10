// A module is source, evaluated at load time in an empty sandbox.
// These system records use the same contract as plugins/mgba/plugin.ts.
// The host supplies publisher identity, never this file.
interface SystemRecord {
  id: string
  title: string
}

enum System {
  Gba = "gba",
}

export const name = "catalog"
export const title = "Example catalog"
export const systems: Record<string, SystemRecord> = {
  [System.Gba]: { id: System.Gba, title: "Game Boy Advance" },
}
