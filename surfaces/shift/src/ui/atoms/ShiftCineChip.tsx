import type { ReactNode } from "react"

/** A glanceable metadata pill in the cinematic home (genre, developer, a launch
 * failure reason, a favourite marker, or what the game still has to download).
 * `tone` selects the semantic accent. */
export type ShiftCineChipTone = "favorite" | "reason" | "download" | "attention" | "saves"

export function ShiftCineChip({
  tone,
  children,
}: {
  readonly tone?: ShiftCineChipTone
  readonly children: ReactNode
}) {
  const modifier =
    tone === "favorite" ? " is-fav"
    : tone === "reason" ? " is-reason"
    : tone === "download" ? " is-download"
    : tone === "attention" ? " is-attention"
    : tone === "saves" ? " is-saves"
    : ""
  return <span className={`shift-cine-chip${modifier}`}>{children}</span>
}
