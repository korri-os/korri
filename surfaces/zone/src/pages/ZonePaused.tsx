import "./ZonePaused.css"
import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
} from "@contracts/surface/korri-surface"
import { useRef } from "react"
import { zonePausedStairs, zonePausedStep } from "../plan/paused"
import { useZoneFit } from "../use-zone-fit"
import { useZoneLayerFocus } from "../use-zone-layer-focus"
import { useZonePausedList } from "../use-zone-paused-list"
import { useZonePausedPlan } from "../use-zone-paused-plan"
import { useZoneStairClip } from "../use-zone-stair-clip"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZoneSlab } from "../ui/atoms/ZoneSlab"
import { ZoneWords } from "../ui/atoms/ZoneWords"
import { ZoneDialog } from "../ui/molecules/ZoneDialog"
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
 * behind the host's window. The game stands in grey: the running game's
 * own art drained of colour when Korri names the game and its catalog has
 * art (`art`), else a grey checker of 16 art px squares, as the greyed
 * loop ground in Mania's reference. A red band across the top-left holds
 * Korri's title (PAUSED when it gives none) in caps-3 on a black slab, its
 * letters rising over the slab's top edge. A yellow wedge rises from the
 * bottom-right corner on a 45 degree stair; Korri's own controls, then
 * each plugin group under its blue tag, step down-left along it, each row
 * one cap further left than the one above. The rows stand at caps-2 and
 * step to caps-1 together when the column is too narrow; when they are too
 * many for the wedge, the gaps between them yield first, and only then
 * does the list scroll, the focused row whole and each edge cutting a row
 * through its words (useZonePausedList).
 *
 * A destructive control asks first: the sky dialog over the dithered page
 * with NO lit before YES, the control's label as headline and the value it
 * would take in yellow. B withdraws the question, else returns to the game.
 *
 * Everything this page knows comes in as props: Korri's overlay, the
 * running game's art, and the overlay model's value (the focused id, the
 * question, the lit answer, a range's value while it moves). It reports
 * through callbacks. Every length is from `planPaused` on the art grid.
 */
export function ZonePaused({
  overlay,
  art,
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
  /** The running game's art (wide, else cover), greyed behind the page; the checker when absent. */
  readonly art?: string
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
  const page = useRef<HTMLDivElement | null>(null)
  const wedge = useRef<HTMLDivElement | null>(null)
  const question = useRef<HTMLDivElement | null>(null)
  const heading = useRef<HTMLHeadingElement | null>(null)
  const band = useRef<HTMLElement | null>(null)
  const rows = useRef<HTMLDivElement | null>(null)
  useZonePausedPlan(page)
  useZoneStairClip(band, { left: "straight" })
  useZoneLayerFocus(wedge, asking === undefined ? focusedId : undefined)
  useZoneLayerFocus(question, asking === undefined ? undefined : `answer:${answer}`)

  const title = overlay.title ?? "Paused"
  const all = [...overlay.controls, ...overlay.groups.flatMap(group => group.controls)]
  const fitKey = `${title}|${all.map(control => control.label).join("|")}`
  useZoneFit(page, [heading, rows], fitKey)
  // After the fit: the gaps yield, then the list stands where its rules put it.
  useZonePausedList(page, wedge, rows, `${fitKey}|${focusedId ?? ""}`)

  const asked = asking === undefined ? undefined : all.find(control => control.id === asking.controlId)

  // Each row and each group's tag takes the next step down the stair.
  const items = overlay.controls.length + overlay.groups.reduce((sum, group) => sum + 1 + group.controls.length, 0)
  const stairs = zonePausedStairs(items)
  let place = 0
  const nextStep = () => {
    const step = zonePausedStep(place, items)
    place += 1
    return step
  }

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
      step={nextStep()}
    />
  )

  const onQuestionFocus = onFocusAnswer === undefined ? undefined : (at: number) => {
    if (at === 0 || at === 1) onFocusAnswer(at)
  }

  return (
    <div className="zone-paused" ref={page}>
      <div aria-hidden="true" className="zone-paused-behind">
        <i className="zone-paused-checker" />
        {art === undefined ? null : <img alt="" className="zone-paused-art" decoding="async" src={art} />}
      </div>
      <header className="zone-paused-band" ref={band}>
        <h1 className="zone-paused-title" ref={heading}>
          <ZoneSlab />
          <span className="zone-paused-title-text">
            <ZoneWords korri={overlay.title !== undefined}>{title}</ZoneWords>
          </span>
        </h1>
      </header>
      <div className="zone-paused-wedge" inert={asking !== undefined}>
        <div className="zone-paused-list" ref={wedge}>
          <div className="zone-paused-rows" data-steps={stairs} ref={rows}>
            {overlay.controls.map(row)}
            {overlay.groups.map(group => (
              <section aria-label={group.label} className="zone-paused-group" key={group.id}>
                <h2 className="zone-paused-group-label" data-step={nextStep()}>
                  <span className="zone-paused-group-tag">
                    <ZoneSlab />
                    <ZoneWords>{group.label}</ZoneWords>
                  </span>
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
