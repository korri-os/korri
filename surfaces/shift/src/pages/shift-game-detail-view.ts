export interface ShiftGameDetailView {
  readonly id: string
  readonly title: string
  readonly artUrl: string
  readonly genre?: string
  readonly developer?: string
  readonly lastPlayedLabel?: string
  readonly playtimeLabel?: string
  readonly favorite?: boolean
  /** What it still has to fetch, already worded ("Downloading 41%"). */
  readonly download?: string
}
