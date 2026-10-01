import { fixtureModel } from "../../fixtures/fixture-host"
import { noArtworkGame } from "../../fixtures/named-states"
import { PicoPartFrame } from "../../fixtures/PicoPartFrame"
import { PICO_ART_CAVERN, PICO_ART_KNIGHT } from "../../fixtures/sample-art"
import { PicoCart } from "./PicoCart"

export const name = "Cart"
export const note = "A cartridge shaped by its art; the shell colour is hashed from the game id"

export const composition = {
  MissingArtHero: [{ part: "src/ui/atoms/PicoCoverArt.atom.part.tsx", state: "MissingArt" }],
} as const

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

// The first shelf cart is selected. Keep its inputs equal to Home's NoArtwork scenario.
export function MissingArtHero() {
  return <PicoCart
    artUrl={noArtworkGame.coverArtUrl}
    id={noArtworkGame.id}
    onActivate={() => undefined}
    placement="hero"
    progress="new"
    resumable={noArtworkGame.resumable ?? false}
    subtitle={noArtworkGame.subtitle}
    title={noArtworkGame.title}
  />
}

export function WideTile() {
  return <PicoCart artUrl={PICO_ART_CAVERN} id="spelunky" placement="tile" title="Spelunky" />
}
