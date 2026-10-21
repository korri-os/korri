/**
 * Shift Settings — a readable page of device facts, in the home's language.
 *
 * Settings is a place you go, not a command about the focused game, so it is a
 * full screen rather than the contextual sheet. It keeps the home's chrome
 * (status bar above, legend below) and its ambient backdrop, so entering
 * settings never feels like leaving Korri.
 *
 * The screen is shorter than the list, so the list moves under a fixed reading
 * band on the same spring the rail uses to keep a tile centered — settings
 * scrolls exactly the way home scrolls, rather than introducing a second feel.
 *
 * Read-only facts offer Back alone. A row becomes a real button, and advertises
 * Select, only when Korri publishes a real interaction for it.
 */
import type {
  SurfaceSettingGroup,
  SurfaceSettingItem,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import { motion } from "framer-motion"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useSurfaceAction } from "../host/surface-host"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftCineKicker } from "../ui/atoms/ShiftCineKicker"
import { ShiftCineTitle } from "../ui/atoms/ShiftCineTitle"
import { ShiftCineBackdrop } from "../ui/molecules/ShiftCineBackdrop"
import { ShiftCineLegend } from "../ui/molecules/ShiftCineLegend"
import { ShiftSettingRow } from "../ui/molecules/ShiftSettingRow"
import { ShiftSheetAction } from "../ui/molecules/ShiftSheetAction"
import { ShiftSettingSheet } from "../ui/organisms/ShiftSettingSheet"
import { ShiftSheetBody } from "../ui/organisms/ShiftSheetBody"
import { ShiftSheetGroup } from "../ui/organisms/ShiftSheetGroup"
import { ShiftSheetHeader } from "../ui/organisms/ShiftSheetHeader"
import { ShiftSheetPanel } from "../ui/organisms/ShiftSheetPanel"
import { ShiftSheetRoot } from "../ui/organisms/ShiftSheetRoot"
import { ShiftSheetTitle } from "../ui/organisms/ShiftSheetTitle"
import {
  ShiftStatusBar,
  type ShiftStatusBarProps,
} from "../ui/molecules/ShiftStatusBar"

/** Same spring as the rail: settings scrolls the way home scrolls. */
const SPRING = { type: "spring", stiffness: 260, damping: 32 } as const

export interface ShiftSettingsProps {
  readonly groups: readonly SurfaceSettingGroup[]
  /** Page subtitle under the kicker. */
  readonly title?: string
  readonly time?: string
  readonly battery?: ShiftStatusBarProps["battery"]
  readonly network?: ShiftStatusBarProps["network"]
  /** Ambient art carried over from the home so the scene stays continuous. */
  readonly backdropArtUrl?: string
  readonly status: SurfaceSettingsStatus
  readonly onChange: (settingId: string, value: string) => void
  readonly onAction: (actionId: string) => void
  readonly onDismissProblem: () => void
  /** Leave settings (B, or the surface's back action). */
  readonly onClose?: () => void
  /** The word over the title. Default "Settings". */
  readonly kicker?: string
  /** One line about the whole page, under its title, in Korri's words. */
  readonly notice?: string
}

/** Flatten groups into the focus order, keeping each row's absolute index so
 * scrolling stays index-driven (group headings are not focusable). */
export function shiftSettingRowIndex(
  groups: readonly SurfaceSettingGroup[],
): readonly { readonly groupTitle: string; readonly itemId: string }[] {
  return groups.flatMap(group =>
    group.items.map(item => ({ groupTitle: group.title, itemId: item.id })),
  )
}

export function ShiftSettings({
  groups,
  title = "This device",
  time,
  battery,
  network,
  backdropArtUrl,
  status,
  onChange,
  onAction,
  onDismissProblem,
  onClose,
  kicker = "Settings",
  notice,
}: ShiftSettingsProps) {
  const [index, setIndex] = useState(0)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [confirmingId, setConfirmingId] = useState<string | null>(null)
  const [trackY, setTrackY] = useState(0)
  const viewportRef = useRef<HTMLDivElement>(null)
  const trackRef = useRef<HTMLDivElement>(null)
  const closeEditor = useCallback(() => {
    setEditingId(null)
    requestAnimationFrame(() => {
      trackRef.current
        ?.querySelector<HTMLElement>(`[data-setting-index="${index}"]`)
        ?.focus({ preventScroll: true })
    })
  }, [index])

  const rows = useMemo(() => shiftSettingRowIndex(groups), [groups])
  const items = useMemo(
    () => groups.flatMap(group => group.items),
    [groups],
  )
  const focusedItem = items[index]
  const focusedItemSaving =
    status._tag === "Saving" && status.settingId === focusedItem?.id
  const focusedItemSelectable = focusedItem?.interaction && !focusedItemSaving
  const editingItem: SurfaceSettingItem | null =
    items.find(item => item.id === editingId) ?? null
  const confirmingItem: SurfaceSettingItem | null =
    items.find(item => item.id === confirmingId) ?? null
  const confirmingAction =
    confirmingItem?.interaction?.kind === "action"
      ? confirmingItem.interaction
      : null

  useSurfaceAction("back", () => {
    if (confirmingId) setConfirmingId(null)
    else if (editingId) closeEditor()
    else onClose?.()
  })

  // Keep the focused row in the reading band: shift the whole list so the
  // focused row's center lands at the viewport's center (the band is fixed,
  // the list moves) — the rail's behaviour, on the vertical axis.
  useEffect(() => {
    const recenter = () => {
      const track = trackRef.current
      const viewport = viewportRef.current
      if (!track || !viewport) return
      const row = track.querySelector<HTMLElement>(
        `[data-setting-index="${index}"]`,
      )
      if (!row) return
      const wanted =
        viewport.clientHeight / 2 - (row.offsetTop + row.offsetHeight / 2)
      // Never scroll past the ends: a short list stays put rather than
      // floating away from the top.
      const min = Math.min(0, viewport.clientHeight - track.scrollHeight)
      setTrackY(Math.max(min, Math.min(0, wanted)))
    }
    recenter()
    window.addEventListener("resize", recenter)
    return () => window.removeEventListener("resize", recenter)
  }, [index])

  // Seed focus on the first row so directional input has a starting point.
  useEffect(() => {
    trackRef.current
      ?.querySelector<HTMLElement>('[data-setting-index="0"]')
      ?.focus({ preventScroll: true })
  }, [])

  const focusRow = useCallback((position: number) => setIndex(position), [])

  let position = -1

  return (
    <div
      data-shift-settings
      className="shift-cine intrinsic relative h-full w-full overflow-hidden"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.settingsTemplate)}
    >
      <ShiftCineBackdrop artUrl={backdropArtUrl ?? ""} />

      <ShiftStatusBar time={time} battery={battery} network={network} />

      <div className="shift-settings-stage">
        <div className="shift-settings-head">
          <ShiftCineKicker>{kicker}</ShiftCineKicker>
          <ShiftCineTitle>{title}</ShiftCineTitle>
          {notice === undefined ? null : <p className="shift-settings-notice" role="status">{notice}</p>}
        </div>

        <div className="shift-settings-viewport" ref={viewportRef}>
          <motion.div
            className="shift-settings-track"
            ref={trackRef}
            animate={{ y: trackY }}
            transition={SPRING}
          >
            {groups.map(group => (
              <div
                key={group.title}
                className="shift-setting-group"
                {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.settingGroup)}
              >
                <span className="shift-setting-group-title">{group.title}</span>
                <div className="shift-setting-group-rows">
                  {group.items.map(item => {
                    position += 1
                    const slot = position
                    const saving =
                      status._tag === "Saving" && status.settingId === item.id
                    return (
                      <ShiftSettingRow
                        key={item.id}
                        index={slot}
                        label={item.label}
                        focused={slot === index}
                        saving={saving}
                        {...(item.value === undefined
                          ? {}
                          : { value: item.value })}
                        {...(item.description === undefined
                          ? {}
                          : { description: item.description })}
                        {...(item.source === undefined ? {} : { source: item.source })}
                        deviceOnly={item.scope === "device"}
                        {...(item.interaction === undefined || saving
                          ? {}
                          : {
                              onSelect: () => {
                                if (item.interaction?.kind === "action") {
                                  if (item.interaction.confirmation) {
                                    setConfirmingId(item.id)
                                  } else {
                                    onAction(item.interaction.actionId)
                                  }
                                } else {
                                  setEditingId(item.id)
                                }
                              },
                            })}
                        onFocus={() => focusRow(slot)}
                      />
                    )
                  })}
                </div>
              </div>
            ))}
          </motion.div>
        </div>

        <div className="shift-settings-footer">
          {status._tag === "Problem" && !editingId ? (
            <button
              type="button"
              className="shift-settings-problem"
              onClick={onDismissProblem}
            >
              {status.message}
            </button>
          ) : null}
          <ShiftCineLegend
            hints={[
              ...(focusedItemSelectable
                ? [{ glyph: "A", label: "Select", primary: true }]
                : []),
              {
                glyph: "B",
                label: "Back",
                primary: !focusedItemSelectable,
              },
            ]}
          />
        </div>
      </div>

      <ShiftSettingSheet
        item={editingItem}
        status={status}
        onChange={value => {
          if (editingItem) onChange(editingItem.id, value)
        }}
        onDismissProblem={onDismissProblem}
        onClose={closeEditor}
      />

      {confirmingAction?.confirmation ? (
        <ShiftSheetRoot
          open
          onClose={() => setConfirmingId(null)}
          label={confirmingAction.confirmation.title}
        >
          <ShiftSheetPanel>
            <ShiftSheetHeader>
              <ShiftSheetTitle>
                {confirmingAction.confirmation.title}
              </ShiftSheetTitle>
            </ShiftSheetHeader>
            <ShiftSheetBody>
              <ShiftSheetGroup title="Confirm">
                <p className="shift-setting-problem">
                  {confirmingAction.confirmation.message}
                </p>
                <ShiftSheetAction
                  label={confirmingAction.confirmation.confirmLabel}
                  onSelect={() => {
                    onAction(confirmingAction.actionId)
                    setConfirmingId(null)
                  }}
                />
                <ShiftSheetAction label="Cancel" onSelect={() => setConfirmingId(null)} />
              </ShiftSheetGroup>
            </ShiftSheetBody>
          </ShiftSheetPanel>
        </ShiftSheetRoot>
      ) : null}

      {rows.length === 0 ? (
        <div className="shift-settings-empty">
          Korri has nothing to report about this device yet.
        </div>
      ) : null}
    </div>
  )
}
