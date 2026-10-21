/**
 * Shift split review — every game whose saves are split between devices
 * (ADR 0014: the first-sync review, and any split found later).
 *
 * The list is the catalog's own facts: each game whose `saves` mark is
 * `Split`. Byte-identical games never carry that mark, so they never appear.
 * A row runs the mark's own host action (`saves.choose`), and the host answers
 * with the split choice. Shift does not launch the game to get there.
 */
import type { SurfaceGame } from "@contracts/surface/korri-surface"
import { useState } from "react"
import { useSurfaceAction } from "../host/surface-host"
import { SHIFT_DESIGN_PARTS } from "../shift-design-parts"
import { ShiftSaveRow } from "../ui/molecules/ShiftSaveRow"
import { ShiftSavesFrame, ShiftSavesState } from "../ui/templates/ShiftSavesFrame"

export type ShiftSplitGame = SurfaceGame & {
  readonly saves: Extract<NonNullable<SurfaceGame["saves"]>, { _tag: "Split" }>
}

/** Catalog games that need a save choice, in catalog order. */
export function shiftSplitGames(games: readonly SurfaceGame[]): readonly ShiftSplitGame[] {
  return games.filter((game): game is ShiftSplitGame => game.saves?._tag === "Split")
}

export interface ShiftSaveReviewProps {
  readonly games: readonly SurfaceGame[]
  readonly time?: string
  readonly onChoose: (gameId: string, actionId: string) => void
  readonly onBack: () => void
}

export function ShiftSaveReview({ games, time, onChoose, onBack }: ShiftSaveReviewProps) {
  const split = shiftSplitGames(games)
  const [index, setIndex] = useState(0)
  useSurfaceAction("back", onBack)

  const focused = split[index]
  const selectable = focused?.saves.choose.enabled === true
  const empty = split.length === 0

  return (
    <ShiftSavesFrame
      part={SHIFT_DESIGN_PARTS.saveReview}
      attrs={{ "data-shift-save-review": "" }}
      kicker="Saves"
      title={empty ? "All saves match" : "Split saves"}
      {...(empty ? {} : { lede:
          split.length === 1
            ? "This game has different saves on two devices. Pick the one to continue from."
            : `These ${split.length} games have different saves on two devices. Pick one save for each game.` })}
      {...(time === undefined ? {} : { time })}
      focusedIndex={index}
      state={empty ? (
        <ShiftSavesState
          headline="No game needs a choice."
          line="A game shows here when its saves differ between two of your devices."
        />
      ) : undefined}
      hints={[
        ...(selectable ? [{ glyph: "A", label: "Choose", primary: true }] : []),
        { glyph: "B", label: "Back", primary: !selectable },
      ]}
    >
      {split.map((game, slot) => (
        <ShiftSaveRow
          key={game.id}
          index={slot}
          label={game.title}
          facts={game.subtitle ? [game.subtitle] : []}
          focused={slot === index}
          onFocus={() => setIndex(slot)}
          {...(game.saves.choose.enabled
            ? { onSelect: () => onChoose(game.id, game.saves.choose.id) }
            : { disabledReason: game.saves.choose.description ?? "Not available right now." })}
        />
      ))}
    </ShiftSavesFrame>
  )
}
