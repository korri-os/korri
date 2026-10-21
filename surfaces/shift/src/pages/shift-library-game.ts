/**
 * Shift library's source-agnostic tile model, preserved from the original
 * Library surface. Korri's composition root projects honest SurfaceGame facts
 * into it; absent metadata remains absent.
 */
import type { ShiftSaveState } from "../save-sync-labels"

export interface ShiftLibraryGame {
  readonly id: string
  readonly title: string
  readonly artUrl: string
  readonly genre?: string
  readonly developer?: string
  readonly favorite?: boolean
  readonly lastPlayedAt?: number
  readonly playtimeMinutes?: number
  /** The game's save mark (ADR 0014). A split also marks the tile. */
  readonly saves?: ShiftSaveState
}
