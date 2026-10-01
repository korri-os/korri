import { PicoHome } from "./PicoHome"

export const name = "Home"
export const note = "The shelf, with Korri's catalog ready"
export const surface = true

export default function PicoHomePart() {
  return (
    <PicoHome
      clockLabel="10:24"
      onChooseLocation={() => undefined}
      onDismiss={() => undefined}
      mode="shelf"
      onOpenGame={() => undefined}
      onRetry={() => undefined}
      view={{
        _tag: "Shelf",
        games: [
          { id: "celeste", subtitle: "PICO-8 · This device", title: "Celeste Classic" },
          { id: "hollow", resumable: true, subtitle: "GBA · This device", title: "Hollow Knight" },
          { id: "tetris", playCount: 12, subtitle: "GB · zao", title: "Tetris" },
          { id: "zelda-la", resumable: true, subtitle: "GBC · This device", title: "Link's Awakening DX" },
          { id: "smb3", playCount: 3, subtitle: "NES · zao", title: "Super Mario Bros. 3" },
          { id: "earthbound", subtitle: "SNES · This device", title: "EarthBound" },
          { id: "sonic2", subtitle: "MD · zao", title: "Sonic 2" },
        ],
      }}
    />
  )
}
