import { PicoCart } from "./PicoCart"

export const name = "Cart"
export const note = "Label colours and dither are hashed from the game id"

export default function PicoCartPart() {
  return (
    <PicoCart
      id="celeste"
      placement="hero"
      resumable={false}
      subtitle="PICO-8 · This device"
      title="Celeste Classic"
    />
  )
}
