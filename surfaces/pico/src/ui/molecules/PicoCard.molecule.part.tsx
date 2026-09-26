import { PicoCard } from "./PicoCard"

export const name = "Card"
export const note = "A cartridge-shaped card: yellow asks, red warns, blue tells"

export default function PicoCardPart() {
  return <PicoCard kicker="PLAY WHERE?" title="Tetris" tone="ask" />
}
