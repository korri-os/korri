import "./ZoneOptions.css"
import { useEffect, useRef } from "react"
import {
  entryHeading,
  focusedRow,
  optionsPlan,
  ZONE_OPTIONS_WIFI,
  type ZoneOptionsRow,
  type ZoneOptionsSlices,
  type ZoneOptionsView,
} from "../plan/options"
import { ZoneBars } from "../ui/atoms/ZoneBars"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZoneSignal } from "../ui/atoms/ZoneSignal"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import type { ZoneKey } from "../ui/molecules/ZoneKeyboard"
import { ZoneOptionGroup, type ZoneOptionGroupRow } from "../ui/molecules/ZoneOptionGroup"
import { ZoneTabStrip, type ZoneTabItem } from "../ui/molecules/ZoneTabStrip"
import { ZoneOptionsEntry } from "../ui/organisms/ZoneOptionsEntry"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"

const TABS: readonly ZoneTabItem[] = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]

/**
 * A plan row as the group draws it. A Wi-Fi row's signal is drawn, not
 * spelt: the connection as the foot's badge, a network's bars before its
 * strength (Korri's, in Jersey 10) and its words (Zone's, in caps). A
 * disabled row stays focusable so Korri's reason can be read.
 */
function drawnRow(row: ZoneOptionsRow): ZoneOptionGroupRow {
  const signal = row.signal
  const valueNode = signal === undefined ? undefined
    : signal._tag === "Connection"
      ? <span className="zone-options-connection"><ZoneSignal connection={signal.connection} panel={false} /></span>
      : (
        <span className="zone-options-network">
          <ZoneBars strengthPercent={signal.strengthPercent} />
          <span className="zone-options-network-strength">{`${signal.strengthPercent}%`}</span>
          <span className="zone-options-network-words">{signal.words.join(" ")}</span>
        </span>
      )
  return {
    ...row.item,
    ...(valueNode === undefined ? {} : { valueNode }),
    ...(row.disabled ? { disabled: true } : {}),
    ...(row.keepCase?.label === true ? { verbatimLabel: true } : {}),
    ...(row.keepCase?.value === true ? { verbatimValue: true } : {}),
    ...(row.ownDescription === true ? { descriptionCaps: true } : {}),
  }
}

/**
 * The OPTIONS tab: Mania's OPTIONS screen. The active tab, the one
 * yellow-on-blue banner, names the page over the yellow OPTIONS ground. The
 * body is Mania's sky row band, the full width of the screen with a black
 * lip over and under it, and the band is the one scroll: every settings
 * group Korri publishes, in its order, then SYSTEM (Korri's device
 * actions), then WI-FI. Each group's title is the black wedge at the
 * band's left and holds there while its rows pass. Up and down walk the
 * rows across the groups; the focused row scrolls into view, and a row cut
 * off by a lip shows there is more. The tabs never cut a row.
 *
 * Over the page, at most one of: the keyboard, rising from the foot, for a
 * text setting, a secret, a Wi-Fi password or a hidden network; or the blue
 * NO / YES dialog before a confirmed or destructive action. Either traps
 * the cursor (`data-block-exit`) and the rows behind it are inert.
 *
 * A shape: Korri's slices and the page's own state come in as props, and
 * every press goes out through a callback. The page moves DOM focus only to
 * follow its state: onto the focused row when the state names one the
 * cursor is not on (the `system` jump, a row Korri removed), onto the first
 * key when the keyboard opens, onto NO when the dialog opens.
 */
export function ZoneOptions({
  korri,
  view,
  strips,
  band,
  clock,
  device,
  build,
  tabFocused,
  onTab,
  onTabFocus,
  onRowFocus,
  onRowPress,
  onRowStep,
  onKey,
  onKeyFocus,
  onClear,
  onDialogFocus,
  onDialogPress,
}: {
  readonly korri: ZoneOptionsSlices
  readonly view: ZoneOptionsView
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  /** Korri's clockLabel, for the foot's timer. */
  readonly clock?: string
  /** The device's name, when Korri publishes one. */
  readonly device?: string
  /** Korri's buildLabel. */
  readonly build?: string
  /** The tab the cursor is on, when it is on the tabs. */
  readonly tabFocused?: string
  readonly onTab?: (tab: string) => void
  readonly onTabFocus?: (tab: string) => void
  readonly onRowFocus?: (rowId: string) => void
  readonly onRowPress?: (rowId: string) => void
  readonly onRowStep?: (rowId: string, direction: -1 | 1) => void
  readonly onKey?: (key: ZoneKey) => void
  readonly onKeyFocus?: (keyId: string) => void
  readonly onClear?: () => void
  readonly onDialogFocus?: (index: 0 | 1) => void
  readonly onDialogPress?: (index: 0 | 1) => void
}) {
  const plan = optionsPlan(korri)
  const rowId = focusedRow(plan, view.focused)?.id
  const keyboard = view.keyboard
  const dialog = keyboard === undefined ? view.dialog : undefined
  const layer = keyboard !== undefined ? "keyboard" : dialog !== undefined ? "dialog" : "none"

  const body = useRef<HTMLDivElement | null>(null)
  const dialogBox = useRef<HTMLDivElement | null>(null)

  // Follow the state with DOM focus: the row it names, when the cursor is
  // nowhere or on another row of this page. A cursor on the tabs stays.
  useEffect(() => {
    const box = body.current
    if (box === null || layer !== "none" || rowId === undefined) return
    const target = box.querySelector<HTMLButtonElement>(".zone-options-scroll [data-focused] > button")
    const active = document.activeElement
    if (target === null || target === active) return
    if (active === null || active === document.body || box.contains(active)) target.focus({ preventScroll: true })
  }, [rowId, layer])

  // The dialog opens with the cursor on NO.
  const dialogOpen = dialog !== undefined
  useEffect(() => {
    const box = dialogBox.current
    if (!dialogOpen || box === null || box.contains(document.activeElement)) return
    box.querySelectorAll<HTMLButtonElement>("button")[dialog?.focused ?? 0]?.focus({ preventScroll: true })
    // Only on opening: the host moves the cursor after that.
  }, [dialogOpen])

  const heading = keyboard === undefined ? undefined : entryHeading(keyboard.entry)
  // A password asked again after Korri rejected it carries Korri's reason,
  // which the keyboard would otherwise cover on the row.
  const attempt = korri.network?.wifi._tag === "Available" ? korri.network.wifi.attempt : undefined
  const entryProblem = attempt?._tag === "Failed" && heading?.name === attempt.ssid ? attempt.message : undefined

  return (
    <ZoneShell
      band={band}
      foot={(
        <ZoneFoot
          build={build}
          clock={clock}
          connection={korri.network?.connection}
          device={device}
        />
      )}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
      tabs={(
        <ZoneTabStrip
          active="options"
          focused={tabFocused}
          onFocus={onTabFocus}
          onSelect={onTab ?? (() => undefined)}
          tabs={TABS}
        />
      )}
    >
      <div className="zone-options" data-layer={layer} ref={body}>
        <div className="zone-options-band" inert={layer !== "none"}>
          <div className="zone-options-scroll">
            <div className="zone-options-groups">
              {plan.groups.map(group => (
                <div className="zone-options-group" data-group={group.key === ZONE_OPTIONS_WIFI ? "wifi" : undefined} key={group.key}>
                  <ZoneOptionGroup
                    focusedId={rowId}
                    onFocus={onRowFocus}
                    onPress={onRowPress}
                    onStep={onRowStep}
                    rows={group.rows.map(drawnRow)}
                    status={group.status}
                    title={group.title}
                  />
                </div>
              ))}
            </div>
          </div>
        </div>
        {keyboard === undefined || heading === undefined ? null : (
          <div className="zone-options-layer" data-block-exit="true" data-kind="keyboard">
            <ZoneOptionsEntry
              caret={keyboard.caret}
              clearLabel={heading.clearLabel}
              draft={keyboard.draft}
              focusedKey={keyboard.focusedKey}
              layer={keyboard.layer}
              maxLength={heading.maxLength}
              name={heading.name}
              onClear={onClear}
              onFocusKey={onKeyFocus}
              onKey={onKey ?? (() => undefined)}
              placeholder={heading.placeholder}
              problem={entryProblem}
              secret={heading.secret}
              title={heading.title}
            />
          </div>
        )}
        {dialog === undefined ? null : (
          <div
            className="zone-options-layer"
            data-block-exit="true"
            data-kind="dialog"
            ref={dialogBox}
          >
            <div className="zone-options-dialog">
              <ZoneDialog
                actions={[
                  { label: "No", onPress: () => onDialogPress?.(0) },
                  { label: dialog.confirm.confirmLabel, onPress: () => onDialogPress?.(1) },
                ]}
                focused={dialog.focused}
                headline={dialog.confirm.headline}
                lines={dialog.confirm.lines}
                onFocusAction={index => {
                  if (index === 0 || index === 1) onDialogFocus?.(index)
                }}
              />
            </div>
          </div>
        )}
      </div>
    </ZoneShell>
  )
}
