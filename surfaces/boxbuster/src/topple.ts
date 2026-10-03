// Shared, module-level topple state so a writer (legacy's collision code in
// controls.tsx; no writer on main yet) can tip a shelf that the scene (gondola animation) and the
// tapes (spill-to-floor) both react to — without prop-drilling through R3F.

export const TOPPLE_SECS = 0.9

export interface ToppleEntry {
  /** +1 = falling toward +X, -1 = toward -X (away from whoever shoved it). */
  readonly dirSign: number
  /** 0..1 animation progress; advanced by the gondola that owns this index. */
  progress: number
}

const entries = new Map<number, ToppleEntry>()

// 0..1 "shove charge" per gondola while it's being pushed, so the scene can
// wobble it as a warning before it actually goes over.
const stress = new Map<number, number>()

/** Set gondola `gi`'s shove charge (0..1). 0 clears it. */
export function setStress(gi: number, v: number): void {
  if (v <= 0) stress.delete(gi)
  else stress.set(gi, v)
}

export function getStress(gi: number): number {
  return stress.get(gi) ?? 0
}

/** Begin toppling gondola `gi`. No-op (returns false) if already toppling. */
export function startTopple(gi: number, dirSign: number): boolean {
  if (entries.has(gi)) return false
  stress.delete(gi)
  entries.set(gi, { dirSign: Math.sign(dirSign) || 1, progress: 0 })
  return true
}

export function getTopple(gi: number): ToppleEntry | undefined {
  return entries.get(gi)
}

export function toppledGondolas(): IterableIterator<number> {
  return entries.keys()
}
