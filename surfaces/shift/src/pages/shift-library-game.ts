/**
 * Shift library's source-agnostic tile model, preserved from the original
 * Library surface. Korri's composition root projects honest SurfaceGame facts
 * into it; absent metadata remains absent.
 */
export interface ShiftLibraryGame {
  readonly id: string
  readonly title: string
  readonly artUrl: string
  readonly genre?: string
  readonly developer?: string
  readonly favorite?: boolean
  readonly lastPlayedAt?: number
  readonly playtimeMinutes?: number
  /** Saves split between devices: the tile carries a marker. */
  readonly split?: boolean
}
