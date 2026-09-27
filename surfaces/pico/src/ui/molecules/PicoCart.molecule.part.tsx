import { fixtureModel } from "../../fixtures/fixture-host"
import { PicoPartFrame } from "../../fixtures/PicoPartFrame"
import { PICO_ART_CAVERN, PICO_ART_KNIGHT } from "../../fixtures/sample-art"
import { PicoCart } from "./PicoCart"

export const name = "Cart"
export const note = "A cartridge shaped by its art; the shell colour is hashed from the game id"

const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[4] : undefined

export default function PicoCartPart() {
  return (
    <PicoCart
      artUrl={game?.coverArtUrl}
      id="lantern"
      placement="still"
      resumable={false}
      subtitle="PC · This device"
      title="Lantern Keep"
    />
  )
}

export function ResumableHero() {
  return (
    <PicoPartFrame><PicoCart
      artUrl={PICO_ART_KNIGHT}
      id="hollow"
      onActivate={() => undefined}
      placement="hero"
      resumable
      subtitle="Switch · This device"
      title="Hollow Knight"
    /></PicoPartFrame>
  )
}

export function MissingArtSide() {
  return <PicoCart id="petal" onActivate={() => undefined} placement="side" title="Petal Quest" />
}

export function WideTile() {
  return <PicoCart artUrl={PICO_ART_CAVERN} id="spelunky" placement="tile" title="Spelunky" />
}
