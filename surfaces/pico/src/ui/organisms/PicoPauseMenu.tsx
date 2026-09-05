import type { PicoOverlayControlView, PicoOverlayView } from "../../pico-overlay-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoControlRow } from "../molecules/PicoControlRow"

/**
 * Korri's gameplay controls as legacy's pause menu.
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
  onRetry,
}: {
  readonly overlay: PicoOverlayView
  readonly onActivate: (control: PicoOverlayControlView) => void
  readonly onRetry: () => void
}) {
  return (
    <>
      <PicoTitle level={2} size="md" text={overlay.title} />
      {overlay.problem === undefined ? null : (
        <section aria-label={overlay.problem.kicker} className="pico-pause-menu-problem">
          <span className="pico-pause-menu-problem-kicker">{overlay.problem.kicker}</span>
          <p className="pico-pause-menu-problem-reason">{overlay.problem.reason}</p>
          {overlay.problem.canRetry ? <PicoButton label="TRY AGAIN" onPress={onRetry} /> : null}
        </section>
      )}
      <ul className="pico-pause-menu-list">
        {overlay.controls.map((control) => (
          <PicoControlRow control={control} key={control.id} onActivate={() => onActivate(control)} />
        ))}
      </ul>
      {overlay.groups.map((group) => (
        <section aria-label={group.label} className="pico-pause-menu-group" key={group.id}>
          <h3 className="pico-pause-menu-group-label">{group.label}</h3>
          <ul className="pico-pause-menu-list">
            {group.controls.map((control) => (
              <PicoControlRow control={control} key={control.id} onActivate={() => onActivate(control)} />
            ))}
          </ul>
        </section>
      ))}
    </>
  )
}
