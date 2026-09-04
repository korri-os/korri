import { PicoHome } from "./PicoHome"

export const name = "Home"
export const note = "The shelf, with Korri's catalog ready"
export const surface = true

export default function PicoHomePart() {
  return (
    <PicoHome
      clockLabel="10:24"
      onLaunch={() => undefined}
      onRetry={() => undefined}
      view={{
        _tag: "Shelf",
        games: [
          { id: "celeste", subtitle: "PICO-8 · This device", title: "Celeste Classic" },
          { id: "hollow", resumable: true, subtitle: "GBA · This device", title: "Hollow Knight" },
          { id: "tetris", subtitle: "GB · zao", title: "Tetris" },
        ],
      }}
    />
  )
}
