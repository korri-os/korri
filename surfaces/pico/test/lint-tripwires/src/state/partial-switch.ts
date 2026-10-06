// Tripwire: a switch over a union that misses a member (rule 4).
type Pressed = { readonly _tag: "Back" } | { readonly _tag: "System" } | { readonly _tag: "Menu" }

export function count(pressed: Pressed): number {
  let seen = 0
  switch (pressed._tag) { // VIOLATION switch-exhaustiveness-check
    case "Back":
      seen = 1
      break
  }
  return seen
}
