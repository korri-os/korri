import "./ZoneRunWith.css"
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import { runWithPlan, zoneSavedRowId, type ZoneRunWithState } from "../plan/options-run-with"
import { ZoneBanner } from "../ui/atoms/ZoneBanner"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneTag, type ZoneTagKind } from "../ui/atoms/ZoneTag"
import { ZoneActionRow } from "../ui/molecules/ZoneActionRow"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneRunWithRoute } from "../ui/organisms/ZoneRunWithRoute"
import { ZoneWarningLines } from "../ui/organisms/ZoneWarningLines"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"
import { useZoneFit } from "../use-zone-fit"
import { useZoneOptionClips } from "../use-zone-option-row"

/** Each state of Korri's runner choice wears a tag colour Zone already uses
 * for that kind of news: blue for settled, yellow for at work, orange for
 * needs a look, red for stopped, white for not yet. Mint is the ground, so
 * no tag on the ground wears it. */
const STATE_TAGS: Record<ZoneRunWithState, ZoneTagKind> = {
  Loading: "continue",
  Ready: "system",
  Stale: "open",
  Warnings: "open",
  Busy: "here",
  Conflict: "remote",
  Error: "remote",
}

/**
 * RUN WITH, reached from GAME OPTS. for one game: which installed runner
 * plays it. It stands on the OPTIONS ground and band. The banner names the
 * page where the tabs stand on every other page (RUN WITH has no tabs);
 * under it the game's title stands on a black panel at caps-2
 * (caps-1 when it must), and Korri's state for the choice is a tag at the
 * end of that row. Under them, in the band: Korri's warnings for the whole
 * choice (black panels with a red bar), its message on a black panel in
 * Jersey 10 as written, one row per route (program, then the exact build,
 * cut in the middle so its tail shows, whole under the row while it is
 * focused) with SAVED on the route a saved choice picks, and Korri's
 * actions as buttons: each route's under it, the choice's own at the
 * bottom. A saved choice whose
 * runner no route offers (Stale) is drawn as a route of its own: SAVED, the
 * runner's id, and UNAVAILABLE where a build would be.
 *
 * Every id goes back unchanged through `onAction`; the host bound it to
 * this read. Closed renders nothing. The page traps the cursor in its rows
 * (`data-block-exit`) and the way out is the host's own action.
 *
 * A shape: Korri's runner choice and the focused id come in as props;
 * focus and presses go out through callbacks. When Korri's state changes
 * and the cursor is not on the page, it lands on the first control.
 */
export function ZoneRunWith({
  choice,
  focused,
  strips,
  band,
  clock,
  build,
  onFocus,
  onAction,
}: {
  readonly choice: SurfaceRunnerChoice
  /** The route row id or action id the cursor is on. */
  readonly focused?: string
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly clock?: string
  readonly build?: string
  readonly onFocus?: (id: string) => void
  readonly onAction?: (actionId: string) => void
}) {
  const list = useRef<HTMLDivElement | null>(null)
  const head = useRef<HTMLDivElement | null>(null)
  const title = useRef<HTMLSpanElement | null>(null)
  const tag = choice._tag
  const message = choice._tag === "Closed" ? "" : choice.message
  useZoneFit(head, [title], choice._tag === "Closed" ? "" : choice.gameTitle)
  useZoneOptionClips(list, message)
  useEffect(() => {
    const box = list.current
    if (box === null || box.contains(document.activeElement)) return
    box.querySelector<HTMLButtonElement>("button")?.focus({ preventScroll: true })
  }, [tag])

  if (choice._tag === "Closed") return null
  const plan = runWithPlan(choice)
  const run = onAction ?? (() => undefined)

  return (
    <ZoneShell
      band={band}
      foot={clock === undefined && build === undefined ? undefined : <ZoneFoot build={build} clock={clock} />}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
      tabs={<ZoneBanner>Run with</ZoneBanner>}
    >
      <div className="zone-run-with" data-state={choice._tag}>
        <div className="zone-run-with-head" ref={head}>
          <span className="zone-run-with-title" ref={title}>
            <ZonePanel>
              <ZoneCaps>{choice.gameTitle}</ZoneCaps>
            </ZonePanel>
          </span>
          <span className="zone-run-with-state">
            <ZoneTag kind={STATE_TAGS[choice._tag]} label={choice._tag} />
          </span>
        </div>
        <div className="zone-run-with-band">
          <div className="zone-run-with-scroll" data-block-exit="true" ref={list}>
            <div className="zone-run-with-rows">
            <ZoneWarningLines lines={choice.warnings} />
            {choice.message === "" ? null : (
              <p className="zone-run-with-message" data-option-clip="slash">{choice.message}</p>
            )}
            {plan.orphans.map(orphan => (
              <ZoneRunWithRoute
                focused={focused}
                key={`${orphan.label}:${orphan.runnerId}`}
                onAction={run}
                route={{
                  runnerId: orphan.runnerId,
                  systemId: "",
                  program: orphan.runnerId,
                  runnerBuild: "UNAVAILABLE",
                  warnings: [],
                  actions: [],
                }}
                rowId={zoneSavedRowId(orphan.label)}
                saved={[orphan.label]}
              />
            ))}
            {plan.routes.map(entry => (
              <ZoneRunWithRoute
                focused={focused}
                key={entry.id}
                onAction={run}
                onFocus={onFocus}
                route={entry.route}
                rowId={entry.id}
                saved={entry.saved}
              />
            ))}
            {choice.actions.length === 0 ? null : (
              <div className="zone-run-with-actions">
                <ZoneActionRow
                  actions={choice.actions.map(action => ({
                    id: action.id,
                    label: action.label,
                    ...(action.enabled ? {} : { disabled: true }),
                    ...(action.description === undefined ? {} : { reason: action.description }),
                  }))}
                  focused={focused}
                  onFocus={onFocus}
                  onPress={run}
                />
              </div>
            )}
            </div>
          </div>
        </div>
      </div>
    </ZoneShell>
  )
}
