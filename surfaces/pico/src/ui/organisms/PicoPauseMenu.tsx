import "./PicoPauseMenu.css"
import type { PicoOverlayControlView, PicoOverlayView, PicoRanging } from "../../pico-overlay-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "../molecules/PicoCard"
import { PicoControlRow } from "../molecules/PicoControlRow"

/**
 * Korri's gameplay controls as a pause menu: a blue card, because it tells
 * you where you are, titled with the game.
 *
 * Korri's own controls first — Resume is always the first thing under the
 * thumb — then each plugin's group under its label.
 *
 * A problem Korri reports goes above the controls, not below them. The menu
 * scrolls, and a failure under the fold means choosing "Save state" without
 * knowing the stream has already dropped. TRY AGAIN appears only when Korri
 * says retrying would do anything.
 */
export function PicoPauseMenu({
  overlay,
  onActivate,
  ranging,
  onRetry,
}: {
  readonly overlay: PicoOverlayView
  readonly onActivate: (control: PicoOverlayControlView) => void
  readonly ranging: PicoRanging
  readonly onRetry: () => void
}) {
  const row = (control: PicoOverlayControlView) => (
    <PicoControlRow
      control={control}
      key={control.id}
      onActivate={() => onActivate(control)}
      onLeave={() => ranging.onLeave(control)}
      onRelease={(ended) => ranging.onRelease(control, ended)}
      onStep={(request) => ranging.onStep(control, request)}
      value={ranging.valueOf(control)}
    />
  )
  return (
    <PicoCard kicker="PAUSED" title={overlay.title} tone="tell">
      {overlay.problem === undefined ? null : (
        <section aria-label={overlay.problem.kicker} className="pico-pause-menu-problem">
          <span className="pico-pause-menu-problem-kicker">{overlay.problem.kicker}</span>
          <p className="pico-pause-menu-problem-reason">{overlay.problem.reason}</p>
          {overlay.problem.canRetry ? <PicoRow label="TRY AGAIN" onPress={onRetry} /> : null}
        </section>
      )}
      <ul className="pico-pause-menu-list">
        {overlay.controls.map(row)}
      </ul>
      {overlay.groups.map((group) => (
        <section aria-label={group.label} className="pico-pause-menu-group" key={group.id}>
          <h3 className="pico-pause-menu-group-label">{group.label}</h3>
          <ul className="pico-pause-menu-list">
            {group.controls.map(row)}
          </ul>
        </section>
      ))}
    </PicoCard>
  )
}
