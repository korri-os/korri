import { PicoCartShelf } from "./PicoCartShelf"

export const name = "Cart Shelf"
export const note = "Focus moves the hero; the caption follows"

export default function PicoCartShelfPart() {
  return (
    <PicoCartShelf
      games={[
        { id: "celeste", subtitle: "PICO-8 · This device", title: "Celeste Classic" },
        { id: "hollow", resumable: true, subtitle: "GBA · This device", title: "Hollow Knight" },
        { id: "tetris", subtitle: "GB · zao", title: "Tetris" },
      ]}
      onOpen={() => undefined}
    />
  )
}
