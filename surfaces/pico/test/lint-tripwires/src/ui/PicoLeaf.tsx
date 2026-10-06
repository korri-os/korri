// Tripwire: a leaf not on the allow list keeps state (rule 3), by name or
// through the React namespace.
import * as React from "react" // VIOLATION no-restricted-imports
import { useReducer } from "react" // VIOLATION no-restricted-imports

export function PicoLeaf(): number {
  const [count] = useReducer((value: number) => value + 1, 0)
  const [other] = React.useState(0)
  return count + other
}
