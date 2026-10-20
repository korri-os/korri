import { ZoneSlab } from "./ZoneSlab"

export const name = "Slab"
export const note = "The black 45 degree slab behind a control's words, on the pink drop when lit"

/** The slab alone, filling the part. */
export default function Default() {
  return <ZoneSlab />
}

/** Lit: the pink drop under it. */
export function Lit() {
  return <ZoneSlab lit />
}
