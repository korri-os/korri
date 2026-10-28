// A module is source, evaluated at load time in an empty sandbox.
// The host supplies publisher identity, never this file.
interface SystemRecord {
  title: string
}

enum System {
  Gba = "gba",
}

export const name = "catalog"
export const title = "Example catalog"
export const systems: Record<string, SystemRecord> = {
  [System.Gba]: { title: "Game Boy Advance" },
}
