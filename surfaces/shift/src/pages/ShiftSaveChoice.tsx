/**
 * Shift split choice — required before a game with split saves launches
 * (ADR 0014; `SurfaceStatus` `ChooseSave`).
 *
 * It keeps the home's scene: the game's art is the backdrop, the status bar
 * stays on top and the legend maps the buttons. In place of the rail it lays
 * the two sides of the split out as cards, so the person can compare where each
 * save came from before they continue.
 *
 * Every pick and every other action is the host's: a card hands back its
 * side's `actionId`, a footer button hands back its action's id, both through
 * `runAction`. Back runs the host's action only when the host offers exactly
 * one, because Shift never decides from a label which action means "not now".
 */
import type {
  SurfaceAction,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { useEffect, useRef, useState } from "react"
import { useSurfaceAction } from "../host/surface-host"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftCineKicker } from "../ui/atoms/ShiftCineKicker"
import { ShiftCineTitle } from "../ui/atoms/ShiftCineTitle"
import { ShiftCineBackdrop } from "../ui/molecules/ShiftCineBackdrop"
import { ShiftCineLegend } from "../ui/molecules/ShiftCineLegend"
import { ShiftSaveSideCard } from "../ui/molecules/ShiftSaveSideCard"
import { ShiftStatusBar } from "../ui/molecules/ShiftStatusBar"

export type ShiftChooseSaveStatus = Extract<SurfaceStatus, { _tag: "ChooseSave" }>

export interface ShiftSaveChoiceProps {
  readonly status: ShiftChooseSaveStatus
  /** The game's title; absent when the catalog does not hold the game. */
  readonly gameTitle?: string
  readonly backdropArtUrl?: string
  readonly time?: string
  readonly now?: Date
  readonly onAction: (actionId: string) => void
}

/** The one host action Back may run, or none when there is a real choice. */
export function soleHostAction(
  actions: readonly SurfaceAction[],
): SurfaceAction | undefined {
  const enabled = actions.filter(action => action.enabled)
  return actions.length === 1 && enabled.length === 1 ? enabled[0] : undefined
}

export function ShiftSaveChoice({
  status,
  gameTitle,
  backdropArtUrl,
  time,
  now,
  onAction,
}: ShiftSaveChoiceProps) {
  const [focus, setFocus] = useState<{ kind: "side" | "action"; index: number }>({
    kind: "side",
    index: 0,
  })
  const sidesRef = useRef<HTMLDivElement>(null)
  const back = soleHostAction(status.actions)

  useSurfaceAction("back", () => {
    if (back) onAction(back.id)
  })

  // A new question (another game, or new sides) starts on its first card, so
  // one confirm never picks a side the person has not looked at.
  const sidesKey = status.sides.map(side => side.actionId).join("|")
  useEffect(() => {
    setFocus({ kind: "side", index: 0 })
    sidesRef.current
      ?.querySelector<HTMLElement>("[data-save-side]")
      ?.focus({ preventScroll: true })
  }, [status.gameId, sidesKey])

  const focusedAction =
    focus.kind === "action" ? status.actions[focus.index] : undefined
  const hints = [
    focus.kind === "side"
      ? { glyph: "A", label: "Continue from this", primary: true }
      : focusedAction?.enabled
        ? { glyph: "A", label: focusedAction.label, primary: true }
        : undefined,
    back ? { glyph: "B", label: back.label } : undefined,
  ].filter(hint => hint !== undefined)

  return (
    <div
      data-shift-saves
      data-shift-save-choice={status.gameId}
      className="shift-cine intrinsic relative h-full w-full overflow-hidden"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.saveChoice)}
    >
      <ShiftCineBackdrop artUrl={backdropArtUrl ?? ""} cooled />
      <ShiftStatusBar time={time} />

      <div className="shift-saves-stage shift-save-choice-stage">
        <header className="shift-saves-head" role="status" aria-live="polite">
          <ShiftCineKicker tone="attention">{status.kicker}</ShiftCineKicker>
          {gameTitle ? <ShiftCineTitle>{gameTitle}</ShiftCineTitle> : null}
          <p className="shift-saves-lede">
            Pick the save to continue from. The other stays in the save history.
          </p>
          {status.detail ? (
            <p className="shift-saves-detail">{status.detail}</p>
          ) : null}
        </header>

        <div
          ref={sidesRef}
          className="shift-save-sides"
          role="group"
          aria-label="Saves to continue from"
        >
          {status.sides.map((side, index) => (
            <ShiftSaveSideCard
              key={side.actionId}
              actionId={side.actionId}
              device={side.device}
              endedAt={side.endedAt}
              durationSeconds={side.durationSeconds}
              {...(side.imageUrl === undefined ? {} : { imageUrl: side.imageUrl })}
              {...(now === undefined ? {} : { now })}
              focused={focus.kind === "side" && focus.index === index}
              onFocus={() => setFocus({ kind: "side", index })}
              onPick={onAction}
            />
          ))}
        </div>

        <footer className="shift-saves-footer">
          <div className="shift-saves-actions">
            {status.actions.map((action, index) => (
              <button
                key={action.id}
                type="button"
                className="shift-saves-action"
                data-tone={action.destructive ? "danger" : undefined}
                disabled={!action.enabled}
                onFocus={() => setFocus({ kind: "action", index })}
                onClick={() => onAction(action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
          {hints.length > 0 ? <ShiftCineLegend hints={hints} /> : null}
        </footer>
      </div>
    </div>
  )
}
