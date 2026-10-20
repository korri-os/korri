import "./ZonePausedControl.css"
import type { SurfaceGameplayControl } from "@contracts/surface/korri-surface"
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef } from "react"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneTriangle } from "../atoms/ZoneTriangle"
import { ZoneUnderline } from "../atoms/ZoneUnderline"

/* The portal delivers left and right to the focused element marked
 * data-korri-horizontal-control as these DOM events instead of moving focus
 * (clients/portal/src/input/spatial-focus.ts). Arrow keys are handled too,
 * for a keyboard with no portal in front of it. */
const DIRECTION_EVENT = "korri-semantic-direction"

const wayOf = (direction: unknown): -1 | 1 | undefined =>
  direction === "left" || direction === "ArrowLeft" ? -1 : direction === "right" || direction === "ArrowRight" ? 1 : undefined

/** A range's value as a number to the step's precision. */
function rangeText(value: number, step: number): string {
  const text = String(step)
  const places = text.includes(".") ? text.length - text.indexOf(".") - 1 : 0
  return value.toFixed(places)
}

/**
 * One row of PAUSED's yellow wedge: a black parallelogram with the
 * control's label in white caps, as CONTINUE, RESTART and EXIT stand in
 * Mania's pause menu. What the right of the row shows is Korri's
 * interaction, verbatim:
 *
 *  - command: nothing; A invokes.
 *  - toggle: Korri's trueLabel or falseLabel; left, right and A flip it.
 *  - choice: the chosen option's label; left and right step it.
 *  - range: the number and a parallelogram meter; left and right step it,
 *    and while it moves the number turns yellow.
 *
 * The focused row wears the pink underline, and a row that steps shows the
 * red triangles either side of its value, dimmed at a range's ends. Their
 * room is kept while unfocused, so focus never moves a row's layout. A
 * disabled row stays focusable, dims, and shows Korri's reason under it; a
 * focused row shows Korri's description. Focus is a prop.
 */
export function ZonePausedControl({
  control,
  rangeValue,
  moving = false,
  focused = false,
  onFocus,
  onPress,
  onStep,
}: {
  readonly control: SurfaceGameplayControl
  /** The range's value to show: the player's while it moves, else Korri's. */
  readonly rangeValue?: number
  readonly moving?: boolean
  readonly focused?: boolean
  readonly onFocus?: () => void
  readonly onPress?: () => void
  readonly onStep?: (direction: -1 | 1) => void
}) {
  const button = useRef<HTMLButtonElement | null>(null)
  const meter = useRef<HTMLSpanElement | null>(null)
  const step = useRef(onStep)
  useEffect(() => {
    step.current = onStep
  })

  const interaction = control.interaction
  const steps = interaction.kind !== "command" && control.enabled

  useEffect(() => {
    const node = button.current
    if (node === null || !steps) return
    const onDirection = (event: Event) => {
      const way = wayOf((event as CustomEvent<{ readonly direction?: string }>).detail?.direction)
      if (way !== undefined) step.current?.(way)
    }
    node.addEventListener(DIRECTION_EVENT, onDirection)
    return () => node.removeEventListener(DIRECTION_EVENT, onDirection)
  }, [steps])

  useEffect(() => {
    if (focused) button.current?.scrollIntoView({ block: "nearest" })
  }, [focused])

  const onKeyDown = steps ? (event: KeyboardEvent<HTMLButtonElement>) => {
    const way = wayOf(event.key)
    if (way === undefined) return
    event.preventDefault()
    step.current?.(way)
  } : undefined

  const value = interaction.kind === "range" ? rangeValue ?? interaction.value : undefined
  const span = interaction.kind === "range" ? interaction.max - interaction.min : 0
  const share = interaction.kind === "range" && value !== undefined
    ? span <= 0 ? 1 : Math.min(1, Math.max(0, (value - interaction.min) / span)) : 0
  // The meter's fill is a measurement written to the DOM, not state.
  useLayoutEffect(() => {
    meter.current?.style.setProperty("--zone-paused-control-share", String(share))
  }, [share])
  const atMin = interaction.kind === "range" && value !== undefined && value <= interaction.min
  const atMax = interaction.kind === "range" && value !== undefined && value >= interaction.max
  const shown = interaction.kind === "toggle" ? (interaction.value ? interaction.trueLabel : interaction.falseLabel)
    : interaction.kind === "choice" ? interaction.options.find(option => option.value === interaction.value)?.label ?? interaction.value
    : interaction.kind === "range" && value !== undefined ? rangeText(value, interaction.step)
    : undefined

  return (
    <div
      className="zone-paused-control"
      data-destructive={control.destructive || undefined}
      data-disabled={!control.enabled || undefined}
      data-focused={focused || undefined}
      data-kind={interaction.kind}
    >
      <button
        aria-disabled={control.enabled ? undefined : "true"}
        className="zone-paused-control-button"
        data-korri-horizontal-control={steps ? interaction.kind : undefined}
        onClick={control.enabled ? onPress : undefined}
        onFocus={onFocus}
        onKeyDown={onKeyDown}
        ref={button}
        type="button"
      >
        <ZonePanel edge={false}>
          <span className="zone-paused-control-face">
            <span className="zone-paused-control-label">{control.label}</span>
            {shown === undefined ? null : (
              <span className="zone-paused-control-value" data-moving={moving || undefined}>
                {steps ? (
                  <span className="zone-paused-control-step" data-shown={focused || undefined}>
                    <ZoneTriangle dimmed={atMin} direction="left" nudge={false} size="inline" />
                  </span>
                ) : null}
                <span className="zone-paused-control-text">{shown}</span>
                {interaction.kind === "range" ? (
                  <span aria-hidden="true" className="zone-paused-control-meter" ref={meter}>
                    <i className="zone-paused-control-fill" />
                  </span>
                ) : null}
                {steps ? (
                  <span className="zone-paused-control-step" data-shown={focused || undefined}>
                    <ZoneTriangle dimmed={atMax} direction="right" nudge={false} size="inline" />
                  </span>
                ) : null}
              </span>
            )}
          </span>
          {focused ? <ZoneUnderline /> : null}
        </ZonePanel>
      </button>
      {!control.enabled && control.disabledReason !== undefined ? (
        <span className="zone-paused-control-note" data-note="reason">{control.disabledReason}</span>
      ) : null}
      {focused && control.enabled && control.description !== undefined ? (
        <span className="zone-paused-control-note" data-note="description">{control.description}</span>
      ) : null}
    </div>
  )
}
