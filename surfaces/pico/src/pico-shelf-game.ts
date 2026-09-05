/**
 * Somewhere a game can be launched from. Mirrors the treaty's shape because it
 * is handed straight back to Korri: the id is opaque and travels unchanged.
 */
export interface PicoShelfLocation {
  readonly id: string
  readonly label: string
}

/**
 * A game as the shelf needs it.
 *
 * Deliberately narrower than the treaty's `SurfaceGame`: Pico's home shows a
 * title, a provenance line, art, whether confirming resumes, and where it can
 * run. Everything else the treaty carries belongs to screens that actually
 * present it, and passing the whole treaty type down would let any component
 * reach for a fact its design never accounted for.
 */
export interface PicoShelfGame {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
  /** Wide art for the shelf's backdrop. Absent for most games. */
  readonly wideArtUrl?: string
  readonly resumable?: boolean
  /**
   * The caption Korri grouped this game under. Absent when Korri grouped
   * nothing — a shelf that invented "All games" would be labelling its own
   * guess as Korri's word.
   */
  readonly section?: string
  /**
   * Present only when Korri says there is a real choice. Pico must ask; picking
   * the first one silently would start a game on the wrong machine, which is
   * the one launch mistake a user cannot undo from the couch.
   */
  readonly locations?: readonly PicoShelfLocation[]
}
