import "./ZonePaused.css"
import "../zone-motion.css"
import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
} from "@contracts/surface/korri-surface"
import { useRef } from "react"
import { useZoneLayerFocus } from "../use-zone-layer-focus"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZonePausedControl } from "../ui/organisms/ZonePausedControl"

/** A destructive control waiting on NO / YES, and the value it would send. */
export interface ZonePausedQuestion {
  readonly controlId: string
  readonly value?: SurfaceGameplayControlValue
}

/** What a question's yellow line says: the value the control would take. */
function valueText(control: SurfaceGameplayControl, value: SurfaceGameplayControlValue | undefined): string | undefined {
  const interaction = control.interaction
  if (value === undefined) return undefined
  if (value.kind === "toggle" && interaction.kind === "toggle") return value.value ? interaction.trueLabel : interaction.falseLabel
  if (value.kind === "choice" && interaction.kind === "choice") {
    return interaction.options.find(option => option.value === value.value)?.label ?? value.value
  }
  return value.kind === "range" ? String(value.value) : undefined
}

/**
 * PAUSED (C24): Mania's pause screen over a game that is still there,
 * behind the host's window. The ground stands in for it, greyed and
 * striped, as Mania greys the stage. A red band across the top-left holds
 * Korri's title (PAUSED when it gives none) on a black parallelogram; a
 * yellow wedge on the right holds Korri's own controls, then each plugin
 * group under its label. The wedge scrolls when the rows outgrow it, and
 * the focused row brings itself into view.
 *
 * A destructive control asks first: Mania's blue dialog with NO lit before
 * YES, the control's label as headline and the value it would take in
 * yellow. B withdraws the question, else returns to the game.
 *
 * Everything this page knows comes in as props: Korri's overlay, and the
 * overlay model's value (the focused id, the question, the lit answer, a
 * range's value while it moves). It reports through callbacks.
 */
export function ZonePaused({
  overlay,
  focusedId,
  asking,
  answer = 0,
  moving = {},
  onFocusControl,
  onPressControl,
  onStepControl,
  onFocusAnswer,
  onAnswer,
}: {
  readonly overlay: SurfaceGameplayOverlayPresentation
  readonly focusedId?: string
  readonly asking?: ZonePausedQuestion
  /** The lit button of the question: 0 is NO, 1 is YES. */
  readonly answer?: 0 | 1
  readonly moving?: Readonly<Record<string, number>>
  readonly onFocusControl?: (controlId: string) => void
  readonly onPressControl?: (controlId: string) => void
  readonly onStepControl?: (controlId: string, direction: -1 | 1) => void
  readonly onFocusAnswer?: (index: 0 | 1) => void
  readonly onAnswer?: (yes: boolean) => void
}) {
  const wedge = useRef<HTMLDivElement | null>(null)
  const question = useRef<HTMLDivElement | null>(null)
  useZoneLayerFocus(wedge, asking === undefined ? focusedId : undefined)
  useZoneLayerFocus(question, asking === undefined ? undefined : `answer:${answer}`)

  const asked = asking === undefined ? undefined
    : [...overlay.controls, ...overlay.groups.flatMap(group => group.controls)].find(control => control.id === asking.controlId)

  const row = (control: SurfaceGameplayControl) => (
    <ZonePausedControl
      control={control}
      focused={control.id === focusedId}
      key={control.id}
      moving={moving[control.id] !== undefined}
      onFocus={onFocusControl === undefined ? undefined : () => onFocusControl(control.id)}
      onPress={onPressControl === undefined ? undefined : () => onPressControl(control.id)}
      onStep={onStepControl === undefined ? undefined : (direction) => onStepControl(control.id, direction)}
      rangeValue={moving[control.id]}
    />
  )

  const onQuestionFocus = onFocusAnswer === undefined ? undefined : (at: number) => {
    if (at === 0 || at === 1) onFocusAnswer(at)
  }

  return (
    <div className="zone-paused">
      <div aria-hidden="true" className="zone-paused-behind">
        <ZoneGround />
        <i className="zone-paused-stripes" />
      </div>
      <header className="zone-paused-band">
        <h1 className="zone-paused-title">
          <ZonePanel edge={false}>
            <ZoneCaps>{overlay.title ?? "Paused"}</ZoneCaps>
          </ZonePanel>
        </h1>
      </header>
      <div className="zone-paused-wedge" inert={asking !== undefined}>
        <div className="zone-paused-list" ref={wedge}>
          <div className="zone-paused-rows">
            {overlay.controls.map(row)}
            {overlay.groups.map(group => (
              <section aria-label={group.label} className="zone-paused-group" key={group.id}>
                <h2 className="zone-paused-group-label">
                  <ZonePanel edge={false} tone="blue">
                    <span className="zone-paused-group-text">{group.label}</span>
                  </ZonePanel>
                </h2>
                {group.controls.map(row)}
              </section>
            ))}
          </div>
        </div>
      </div>
      <span className="zone-paused-hint">
        <ZoneKeycap button="B" word="Back" />
      </span>
      {asked === undefined ? null : (
        <div className="zone-paused-question" data-block-exit ref={question}>
          <ZoneDialog
            actions={[
              { label: "No", onPress: onAnswer === undefined ? undefined : () => onAnswer(false) },
              { label: "Yes", onPress: onAnswer === undefined ? undefined : () => onAnswer(true) },
            ]}
            focused={answer}
            headline={asked.label}
            lines={asked.description === undefined ? [] : [asked.description]}
            onFocusAction={onQuestionFocus}
            title={valueText(asked, asking?.value)}
          />
        </div>
      )}
    </div>
  )
}
