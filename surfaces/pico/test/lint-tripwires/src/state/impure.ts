// Tripwire: each line marked VIOLATION must be reported by oxlint (rule 1).
import { useState } from "react" // VIOLATION no-restricted-imports
import type { SurfaceHost } from "@contracts/surface/korri-surface" // VIOLATION no-restricted-imports

export const hook = useState
export type Holds = SurfaceHost
export const title = (): string => document.title // VIOLATION no-restricted-globals
export const later = (): void => { setTimeout(() => undefined, 1) } // VIOLATION no-restricted-globals
export const now = (): number => Date.now() // VIOLATION no-restricted-globals
