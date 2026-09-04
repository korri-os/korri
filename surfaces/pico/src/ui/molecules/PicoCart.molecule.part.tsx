import { PicoCart } from "./PicoCart"

export const name = "Cart"
export const note = "Focusable cartridge; focus is what makes it the hero"

export default function PicoCartPart() {
  return (
    <PicoCart
      placement="hero"
      resumable={false}
      subtitle="GBA · This device"
      title="Celeste Classic"
    />
  )
}
