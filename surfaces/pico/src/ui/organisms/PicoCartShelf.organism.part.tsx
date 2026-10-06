import { useState } from "react"
import { fixtureGame, longTitleGame, fixtureShelfGames, noArtworkGame } from "../../fixtures/named-states"
import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoCartShelf } from "./PicoCartShelf"

export const composition = {
  NoArtwork: [{ part: "src/ui/molecules/PicoCart.molecule.part.tsx", state: "MissingArtHero" }],
} as const

// Intentionally null: the shelf has no selected game when its input is empty.
export function EmptyLibrary() {
  return <PicoCartShelf games={[]} onOpen={() => undefined} onSelect={() => undefined} selectedId={undefined} />
}

export function NoArtwork() {
  return <PicoCartShelf games={fixtureShelfGames([noArtworkGame])} onOpen={() => undefined} onSelect={() => undefined} selectedId={undefined} />
}

export function ResumableFirst() {
  return <PicoCartShelf games={fixtureShelfGames([fixtureGame("hollow"), longTitleGame])} onOpen={() => undefined} onSelect={() => undefined} selectedId={undefined} />
}

export function LongTitle() {
  return <PicoCartShelf games={fixtureShelfGames([longTitleGame])} onOpen={() => undefined} onSelect={() => undefined} selectedId={undefined} />
}

// The chosen cart is a prop, so a named state can pin any position, and the
// Inspector can set it on the default preview.
export function ThirdCartChosen() {
  const games = view._tag === "Shelf" ? view.games : []
  return <PicoCartShelf games={games} onOpen={() => undefined} onSelect={() => undefined} selectedId={games[2]?.id} />
}

export const name = "Cart Shelf"
export const note = "Carts take their art's height on one baseline; focus moves the stage"

const view = picoHomeViewFromCatalog(fixtureModel.catalog)

export default function PicoCartShelfPart() {
  return <PicoCartShelf games={view._tag === "Shelf" ? view.games : []} onOpen={() => undefined} onSelect={() => undefined} selectedId={undefined} />
}

// The surface keeps the chosen cart; here the part does, so the stage follows
// the d-pad, a pointer and Tab, as it does on the device.
export function Selectable() {
  const games = view._tag === "Shelf" ? view.games : []
  const [chosen, setChosen] = useState<string | undefined>(undefined)
  return <PicoCartShelf games={games} onOpen={() => undefined} onSelect={setChosen} selectedId={chosen} />
}
