import { PicoCard } from "./PicoCard"

export const name = "Card"
export const note = "A cartridge-shaped card: yellow asks, red warns, blue tells"

export default function PicoCardPart() {
  return <PicoCard kicker="PLAY WHERE?" title="Tetris" tone="ask" />
}

export function Warning() {
  return <PicoCard kicker="CANNOT BE UNDONE" title="FORGET EVERYTHING?" tone="warn" />
}

export function ShellInformation() {
  return <PicoCard shell={0} title="DEVICE" titleId="device-card-title" tone="tell" />
}
