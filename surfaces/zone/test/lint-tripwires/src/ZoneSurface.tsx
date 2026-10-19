// Tripwire: the composition root holds no state (rule 3). A ref stays allowed.
import { useRef, useState } from "react" // VIOLATION no-restricted-imports

export function Root(): number {
  const pressed = useRef(false)
  const [count] = useState(0)
  return pressed.current ? count : 0
}
