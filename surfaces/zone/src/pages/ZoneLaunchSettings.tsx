import "./ZoneLaunchSettings.css"
import type { SurfaceLaunchSettings } from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import { launchPlan, ZONE_LAUNCH_SCOPE_ROW, type ZoneLaunchState } from "../plan/launch-settings"
import { entryHeading, focusedRow, isPick, optionsPlan, type ZoneOptionsView } from "../plan/options"
import { ZoneBanner } from "../ui/atoms/ZoneBanner"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneTag, type ZoneTagKind } from "../ui/atoms/ZoneTag"
import { ZoneVeil } from "../ui/atoms/ZoneVeil"
import { ZoneActionRow } from "../ui/molecules/ZoneActionRow"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import type { ZoneKey } from "../ui/molecules/ZoneKeyboard"
import { ZoneOptionGroup } from "../ui/molecules/ZoneOptionGroup"
import { ZoneOptionsEntry } from "../ui/organisms/ZoneOptionsEntry"
import { ZoneOptionRow } from "../ui/molecules/ZoneOptionRow"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"
import { useZoneBandWedge } from "../use-zone-band-wedge"
import { useZoneFit } from "../use-zone-fit"

/** Korri's state for the editor wears the tag colour Zone uses for that
 * kind of news, as on RUN WITH: blue settled, black not yet, red stopped. */
const STATE_TAGS: Record<ZoneLaunchState, ZoneTagKind> = {
  Loading: "continue",
  Ready: "system",
  Conflict: "remote",
  Error: "remote",
}

/**
 * LAUNCH SETTINGS, reached from GAME OPTS. for one game: the values that
 * shape how it runs, on every layer and runner scope Korri publishes. It
 * stands on the OPTIONS ground and band, as RUN WITH does: the banner where
 * the tabs stand, the game's title on a black panel with Korri's state as a
 * tag, then the band. In the band: Zone's RUNNER SCOPE row (ANY RUNNER, a
 * family, each runner), stepped like any choice, which picks the places
 * shown;
 * Korri's lines about the whole editor (places it cannot show yet, a runner
 * that is not installed, a runner whose facts it could not read) or its
 * message (a change made elsewhere, a failure); then one group per layer
 * for the chosen scope, its title the black wedge that holds while its rows
 * pass; then Korri's actions as buttons.
 *
 * The rows are OPTIONS' rows: a choice steps on left, right and A; a number
 * steps on left and right and A raises the keyboard; a profile's name
 * raises the keyboard; ADD SETTING moves a candidate on left and right and
 * A adds it; DELETE PROFILE asks NO / YES in Korri's words. Every value row
 * carries its source, Korri's note and the DEVICE-ONLY tag under it; a row
 * only a file sets draws no control. While Korri reads, or after a conflict
 * or a failure, the rows give way to Korri's message and actions.
 *
 * A shape: Korri's editor, its status and the page's own state come in as
 * props; every press goes out through a callback. DOM focus follows the
 * state: onto the focused row, else the first control when the cursor is
 * nowhere; onto the first key when the keyboard opens; onto NO when the
 * dialog opens.
 */
export function ZoneLaunchSettings({
  launch,
  status,
  scope = 0,
  view,
  focusedAction,
  strips,
  band,
  clock,
  build,
  onRowFocus,
  onRowPress,
  onRowStep,
  onKey,
  onKeyFocus,
  onClear,
  onDialogFocus,
  onDialogPress,
  onAction,
  onActionFocus,
}: {
  readonly launch: Exclude<SurfaceLaunchSettings, { readonly _tag: "Closed" }>
  readonly status: Parameters<typeof launchPlan>[1]
  /** The runner scope shown, by its place on the strip. */
  readonly scope?: number
  readonly view: ZoneOptionsView
  /** The action the cursor is on, when it is on one. */
  readonly focusedAction?: string
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly clock?: string
  readonly build?: string
  readonly onRowFocus?: (rowId: string) => void
  readonly onRowPress?: (rowId: string) => void
  readonly onRowStep?: (rowId: string, direction: -1 | 1) => void
  readonly onKey?: (key: ZoneKey) => void
  /** The clear key beside a number's keyboard (INHERIT), when Korri lets it clear. */
  readonly onClear?: () => void
  readonly onKeyFocus?: (keyId: string) => void
  readonly onDialogFocus?: (index: 0 | 1) => void
  readonly onDialogPress?: (index: 0 | 1) => void
  readonly onAction?: (actionId: string) => void
  readonly onActionFocus?: (actionId: string) => void
}) {
  const launchView = launchPlan(launch, status, scope)
  const plan = optionsPlan(launchView.slices)
  const rowId = focusedRow(plan, view.focused)?.id
  const keyboard = view.keyboard
  const dialog = keyboard === undefined ? view.dialog : undefined
  const layer = keyboard !== undefined ? "keyboard" : dialog !== undefined ? "dialog" : "none"
  const words = launch._tag === "Conflict" || launch._tag === "Error" ? [launch.message, ...launch.notes] : launch.notes
  // Zone's RUNNER SCOPE row stands on its own above the layers, no wedge.
  const scopeRow = plan.groups.flatMap(group => group.rows).find(row => row.id === ZONE_LAUNCH_SCOPE_ROW)?.item
  const groups = plan.groups.filter(group => !group.rows.some(row => row.id === ZONE_LAUNCH_SCOPE_ROW))
  // A pick row shows the candidate A would send, not Korri's "choose" word.
  const pick = view.pick
  const shownRow = (item: (typeof plan.groups)[number]["rows"][number]["item"]) => {
    if (!isPick(item) || item.interaction?.kind !== "choice") return item
    const choices = item.interaction.choices
    const at = pick?.rowId === item.id ? Math.min(pick.at, choices.length - 1) : 0
    return { ...item, value: choices[at]?.label ?? item.value }
  }

  const body = useRef<HTMLDivElement | null>(null)
  const rowBand = useRef<HTMLDivElement | null>(null)
  useZoneBandWedge(rowBand)
  const head = useRef<HTMLDivElement | null>(null)
  const title = useRef<HTMLSpanElement | null>(null)
  useZoneFit(head, [title], launch.gameTitle ?? "")
  const dialogBox = useRef<HTMLDivElement | null>(null)

  // The cursor follows the state onto its row; with no row named and the
  // cursor nowhere, or Korri's state changed under it (Loading to Ready),
  // onto the first control.
  const shownTag = useRef(launch._tag)
  useEffect(() => {
    const box = body.current
    if (box === null || layer !== "none") return
    const changed = shownTag.current !== launch._tag
    shownTag.current = launch._tag
    const active = document.activeElement
    const lost = changed || active === null || active === document.body || !box.contains(active)
    const named = rowId === undefined ? null : box.querySelector<HTMLButtonElement>(".zone-launch-scroll [data-focused] > button")
    if (named !== null && named !== active && (lost || active?.closest(".zone-option-row") !== null)) {
      named.focus({ preventScroll: true })
      return
    }
    if (lost) box.querySelector<HTMLButtonElement>(".zone-launch-scroll button")?.focus({ preventScroll: true })
  }, [rowId, layer, launch._tag])

  const dialogOpen = dialog !== undefined
  useEffect(() => {
    const box = dialogBox.current
    if (!dialogOpen || box === null || box.contains(document.activeElement)) return
    box.querySelectorAll<HTMLButtonElement>("button")[dialog?.focused ?? 0]?.focus({ preventScroll: true })
  }, [dialogOpen])

  const heading = keyboard === undefined ? undefined : entryHeading(keyboard.entry)
  const run = onAction ?? (() => undefined)

  return (
    <ZoneShell
      band={band}
      foot={clock === undefined && build === undefined ? undefined : <ZoneFoot build={build} clock={clock} />}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
      tabs={<ZoneBanner>Launch settings</ZoneBanner>}
    >
      <div className="zone-launch" data-layer={layer} data-state={launch._tag} ref={body}>
        <div className="zone-launch-head" ref={head}>
          {launch.gameTitle === undefined ? null : (
            <span className="zone-launch-title">
              <ZonePanel>
                <span className="zone-launch-title-words" ref={title}>{launch.gameTitle}</span>
              </ZonePanel>
            </span>
          )}
          <span className="zone-launch-state">
            <ZoneTag kind={STATE_TAGS[launch._tag]} label={launch._tag} />
          </span>
        </div>
        <div className="zone-launch-band" inert={layer !== "none"} ref={rowBand}>
          <div className="zone-launch-scroll" data-block-exit="true">
            <div className="zone-launch-groups">
              {scopeRow === undefined ? null : (
                <div className="zone-launch-scopes">
                  <ZoneOptionRow
                    description={scopeRow.description}
                    focused={rowId === ZONE_LAUNCH_SCOPE_ROW}
                    interaction={scopeRow.interaction}
                    label={scopeRow.label}
                    onFocus={() => onRowFocus?.(ZONE_LAUNCH_SCOPE_ROW)}
                    onPress={() => onRowPress?.(ZONE_LAUNCH_SCOPE_ROW)}
                    onStep={direction => onRowStep?.(ZONE_LAUNCH_SCOPE_ROW, direction)}
                    value={scopeRow.value}
                    verbatimValue
                  />
                </div>
              )}
              {words.map((line, index) => (
                // Korri's lines may repeat; a line's place is its identity.
                <p className="zone-launch-message" key={`${index}:${line}`}>{line}</p>
              ))}
              {groups.map((group, index) => (
                <div className="zone-launch-group" key={group.key}>
                  <ZoneOptionGroup
                    focusedId={rowId}
                    followed={index < groups.length - 1}
                    onFocus={onRowFocus}
                    onPress={onRowPress}
                    onStep={onRowStep}
                    rows={group.rows.map(row => ({ ...shownRow(row.item), verbatimValue: true }))}
                    status={group.status}
                    title={group.title}
                  />
                </div>
              ))}
              {launchView.actions.length === 0 ? null : (
                <div className="zone-launch-actions">
                  <ZoneActionRow
                    actions={launchView.actions.map(action => ({
                      id: action.id,
                      label: action.label,
                      ...(action.enabled ? {} : { disabled: true }),
                      ...(action.description === undefined ? {} : { reason: action.description }),
                    }))}
                    focused={focusedAction}
                    onFocus={onActionFocus}
                    onPress={run}
                  />
                </div>
              )}
            </div>
          </div>
        </div>
        {keyboard === undefined || heading === undefined ? null : (
          <div className="zone-launch-layer" data-block-exit="true" data-kind="keyboard">
            <ZoneVeil />
            <ZoneOptionsEntry
              caret={keyboard.caret}
              clearLabel={heading.clearLabel}
              draft={keyboard.draft}
              focusedKey={keyboard.focusedKey}
              layer={keyboard.layer}
              maxLength={heading.maxLength}
              onClear={onClear}
              onFocusKey={onKeyFocus}
              onKey={onKey ?? (() => undefined)}
              placeholder={heading.placeholder}
              secret={heading.secret}
              title={heading.title}
            />
          </div>
        )}
        {dialog === undefined ? null : (
          <div className="zone-launch-layer" data-block-exit="true" data-kind="dialog" ref={dialogBox}>
            <div className="zone-launch-dialog">
              <ZoneDialog
                actions={[
                  { label: "No", onPress: () => onDialogPress?.(0) },
                  { label: dialog.confirm.confirmLabel, onPress: () => onDialogPress?.(1) },
                ]}
                focused={dialog.focused}
                headline={dialog.confirm.headline}
                korriHeadline={dialog.confirm.korriHeadline === true}
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
