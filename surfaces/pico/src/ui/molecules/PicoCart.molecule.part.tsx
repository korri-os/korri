import { fixtureModel } from "../../fixtures/fixture-host"
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
