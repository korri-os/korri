/**
 * Shift save history — one game's save snapshots, newest first, as Korri
 * sends them (`SurfaceModel.saveHistory`, ADR 0014).
 *
 * Each row states the device that made the snapshot, when its session ended by
 * that device's clock, how long it lasted, and whether it is interrupted or the
 * one this device continues from. Shift keeps Korri's order and never sorts by
 * the clocks.
 *
 * Restoring asks first. Cancel comes first and takes the cursor, then the
 * host's own restore action, sent unchanged through `runAction`. Leaving runs
 * the host's action when it offers exactly one; with several, the person picks
 * one in the footer.
 */
import type {
  SurfaceSaveHistory,
  SurfaceSaveHistoryEntry,
} from "@contracts/surface/korri-surface"
import { useState } from "react"
import { useSurfaceAction } from "../host/surface-host"
import {
  formatSaveEndedAt,
  formatSaveEndedAtInSentence,
  formatSavePlayed,
} from "../save-sync-labels"
import { SHIFT_DESIGN_PARTS } from "../shift-design-parts"
import { ShiftSaveRow, type ShiftSaveRowMark } from "../ui/molecules/ShiftSaveRow"
import { ShiftSheetAction } from "../ui/molecules/ShiftSheetAction"
import { ShiftSheetBody } from "../ui/organisms/ShiftSheetBody"
import { ShiftSheetGroup } from "../ui/organisms/ShiftSheetGroup"
import { ShiftSheetHeader } from "../ui/organisms/ShiftSheetHeader"
import { ShiftSheetPanel } from "../ui/organisms/ShiftSheetPanel"
import { ShiftSheetRoot } from "../ui/organisms/ShiftSheetRoot"
import { ShiftSheetTitle } from "../ui/organisms/ShiftSheetTitle"
import { ShiftSavesFrame, ShiftSavesState } from "../ui/templates/ShiftSavesFrame"
import { soleHostAction } from "./ShiftSaveChoice"

export interface ShiftSaveHistoryProps {
  readonly history: SurfaceSaveHistory
  readonly gameTitle?: string
  readonly backdropArtUrl?: string
  readonly time?: string
  readonly now?: Date
  readonly onAction: (actionId: string) => void
}

function entryMarks(entry: SurfaceSaveHistoryEntry): readonly ShiftSaveRowMark[] {
  return [
    ...(entry.current ? [{ label: "Current", tone: "accent" as const }] : []),
    ...(entry.interrupted ? [{ label: "Interrupted", tone: "attention" as const }] : []),
  ]
}

export function ShiftSaveHistory({
  history,
  gameTitle,
  backdropArtUrl,
  time,
  now,
  onAction,
}: ShiftSaveHistoryProps) {
  const [index, setIndex] = useState(0)
  const [footerFocused, setOnActionFocus] = useState(false)
  const [confirming, setConfirming] = useState<SurfaceSaveHistoryEntry | null>(null)
  const back = soleHostAction(history.actions)
  const entries = history._tag === "Ready" ? history.entries : []
  const focused = footerFocused ? undefined : entries[index]
  const restorable = focused?.restore?.enabled === true

  useSurfaceAction("back", () => {
    if (confirming) return
    if (back) onAction(back.id)
  })

  const title = gameTitle ?? "Save history"
  const state =
    history._tag === "Loading" ? (
      <ShiftSavesState headline="Reading the save history…" loading />
    ) : history._tag === "Error" ? (
      <ShiftSavesState tone="danger" headline="Korri could not read the save history" line={history.message} />
    ) : entries.length === 0 ? (
      <ShiftSavesState
        headline="No saves yet"
        line={`Korri keeps a save each time you quit ${gameTitle ?? "this game"}.`}
      />
    ) : undefined

  // Closing the question puts the cursor back on the row it was about.
  const closeConfirm = () => {
    setConfirming(null)
    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(`[data-shift-save-history] [data-save-index="${index}"]`)
        ?.focus({ preventScroll: true })
    })
  }
  const confirmingEnded = confirming ? formatSaveEndedAtInSentence(confirming.endedAt, now) : ""

  return (
    <>
      <ShiftSavesFrame
        part={SHIFT_DESIGN_PARTS.saveHistory}
        attrs={{ "data-shift-save-history": history.gameId }}
        kicker="Save history"
        title={title}
        {...(history._tag === "Ready" && entries.length > 0
          ? { lede: "Newest first. Restoring makes a new save; nothing here is lost." }
          : {})}
        {...(time === undefined ? {} : { time })}
        {...(backdropArtUrl === undefined ? {} : { backdropArtUrl })}
        focusedIndex={index}
        state={state}
        actions={history.actions}
        onAction={onAction}
        onActionFocus={() => setOnActionFocus(true)}
        hints={[
          ...(restorable && focused?.restore
            ? [{ glyph: "A", label: focused.restore.label, primary: true }]
            : []),
          ...(back ? [{ glyph: "B", label: back.label, primary: !restorable }] : []),
        ]}
      >
        {entries.map((entry, slot) => {
          const restore = entry.restore
          return (
            <ShiftSaveRow
              key={entry.snapshotId}
              index={slot}
              label={entry.device}
              facts={[formatSaveEndedAt(entry.endedAt, now), formatSavePlayed(entry.durationSeconds)]}
              marks={entryMarks(entry)}
              focused={!footerFocused && slot === index}
              onFocus={() => {
                setOnActionFocus(false)
                setIndex(slot)
              }}
              {...(restore?.enabled
                ? { onSelect: () => setConfirming(entry) }
                : restore
                  ? { disabledReason: restore.description ?? "Cannot restore right now." }
                  : {})}
            />
          )
        })}
      </ShiftSavesFrame>

      {confirming?.restore ? (
        <ShiftSheetRoot open onClose={closeConfirm} label="Restore this save?">
          <ShiftSheetPanel>
            <ShiftSheetHeader>
              <ShiftSheetTitle>Restore this save?</ShiftSheetTitle>
            </ShiftSheetHeader>
            <ShiftSheetBody>
              <ShiftSheetGroup title={title}>
                <p className="shift-game-action-confirm-message">
                  {`Korri makes a new save from the one ${confirming.device} made ${confirmingEnded}. The save you have now stays in the save history.`}
                </p>
                <ShiftSheetAction label="Cancel" onSelect={closeConfirm} />
                <ShiftSheetAction
                  label={confirming.restore.label}
                  {...(confirming.restore.destructive ? { tone: "danger" as const } : {})}
                  onSelect={() => {
                    const id = confirming.restore?.id
                    closeConfirm()
                    if (id) onAction(id)
                  }}
                />
              </ShiftSheetGroup>
            </ShiftSheetBody>
          </ShiftSheetPanel>
        </ShiftSheetRoot>
      ) : null}
    </>
  )
}
