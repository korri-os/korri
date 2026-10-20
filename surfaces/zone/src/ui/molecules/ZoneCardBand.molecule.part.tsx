import { fixtureCards, fixtureCardsMany } from "../../fixtures/zone-cards"
import { ZoneCardBand } from "./ZoneCardBand"

export const name = "Card band"
export const note = "The focused card centred and large; side cards at the plan's scale; the rest run out"

export default function Default() {
  return <ZoneCardBand cards={fixtureCards} focusedIndex={3} />
}

/** Twelve cards, Spelunky focused: two side cards either side. */
export function TwelveFocusThree() {
  return <ZoneCardBand cards={fixtureCards} focusedIndex={3} />
}

/** The first card focused: nothing to its left, the left triangle dim. */
export function FocusFirst() {
  return <ZoneCardBand cards={fixtureCards} focusedIndex={0} />
}

/** The last card focused: nothing to its right. */
export function FocusLast() {
  return <ZoneCardBand cards={fixtureCards} focusedIndex={11} />
}

/** One card: centred alone, both triangles dim. */
export function OneCard() {
  return <ZoneCardBand cards={fixtureCards.slice(3, 4)} focusedIndex={0} />
}

/** No cards: an empty band with one caps line. */
export function NoCards() {
  return <ZoneCardBand cards={[]} focusedIndex={0} />
}

/** Forty cards, one in the middle focused. */
export function FortyCards() {
  return <ZoneCardBand cards={fixtureCardsMany} focusedIndex={20} />
}

/** The plan's narrow answer: three visible. */
export function Narrow() {
  return <ZoneCardBand cards={fixtureCards} focusedIndex={3} visible={3} />
}

/** Korri's sections: a spine with the caption opens each run; it takes no focus. */
export function Sections() {
  const sectioned = fixtureCards.map((card, index) => ({
    ...card,
    section: index === 0 ? "Continue" : index === 1 ? "This device" : index === 6 ? "zao" : undefined,
  }))
  return <ZoneCardBand cards={sectioned} focusedIndex={1} />
}
