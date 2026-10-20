import "./ZonePausedControl.css"
import type { SurfaceGameplayControl } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { useZoneHorizontal } from "../../use-zone-horizontal"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneSlab } from "../atoms/ZoneSlab"
import { ZoneTriangle } from "../atoms/ZoneTriangle"
import { ZoneWords } from "../atoms/ZoneWords"

/** A range's value as a number to the step's precision. */
function rangeText(value: number, step: number): string {
  const text = String(step)
  const places = text.includes(".") ? text.length - text.indexOf(".") - 1 : 0
  return value.toFixed(places)
}

/**
 * One row of PAUSED's yellow wedge, as CONTINUE, RESTART and EXIT stand in
 * Mania's pause menu: a black 45 degree slab with the control's label in
 * white caps (the danger ink for a destructive one) rising 4 art px above
 * the slab's top edge. What the right of the slab shows is Korri's
 * interaction, its words in Jersey 10 as written:
 *
 *  - command: nothing; A invokes.
 *  - toggle: Korri's trueLabel or falseLabel; left, right and A flip it.
 *  - choice: the chosen option's label; left and right step it.
 *  - range: the number and SPECIAL STAGE's red bar on its dark track; left
 *    and right step it, and while it moves the number turns yellow.
 *
 * The focused row stands on the pink drop, and a row that steps shows the
 * red triangles either side of its value, dimmed at a range's ends. Their
 * room is kept while unfocused, so focus never moves a row's layout. A
 * disabled row stays focusable, dims, and shows Korri's reason under it; a
 * focused row shows Korri's description. Focus is a prop.
 *
 * `step` is the row's place on the wedge's stair: each step stands one cap
 * further left than the row above (ZonePaused counts them).
 */
export function ZonePausedControl({
  control,
  rangeValue,
  moving = false,
  focused = false,
  step = 0,
  onFocus,
  onPress,
  onStep,
}: {
  readonly control: SurfaceGameplayControl
  /** The range's value to show: the player's while it moves, else Korri's. */
  readonly rangeValue?: number
  readonly moving?: boolean
  readonly focused?: boolean
  /** The row's step down-left along the wedge, 0 for the top row. */
  readonly step?: number
  readonly onFocus?: () => void
  readonly onPress?: () => void
  readonly onStep?: (direction: -1 | 1) => void
}) {
  const button = useRef<HTMLButtonElement | null>(null)
  const meter = useRef<HTMLSpanElement | null>(null)
  const fill = useRef<HTMLElement | null>(null)
  const interaction = control.interaction
  const steps = interaction.kind !== "command" && control.enabled

  const horizontal = useZoneHorizontal(button, steps, interaction.kind, onStep)
  useZoneStairClip(meter)
  useZoneStairClip(fill, { left: "straight" })

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
      data-step={step}
    >
      <button
        aria-disabled={control.enabled ? undefined : "true"}
        className="zone-paused-control-button"
        data-korri-horizontal-control={horizontal["data-korri-horizontal-control"]}
        onClick={control.enabled ? onPress : undefined}
        onFocus={onFocus}
        ref={button}
        type="button"
      >
        <ZoneSlab lit={focused} />
        <span className="zone-paused-control-face">
          <span className="zone-paused-control-label">
            <ZoneWords ink={control.destructive && control.enabled ? "danger" : "white"}>{control.label}</ZoneWords>
          </span>
          {shown === undefined ? null : (
            <span className="zone-paused-control-value" data-moving={moving || undefined}>
              {steps ? (
                <span className="zone-paused-control-step" data-shown={focused || undefined}>
                  <ZoneTriangle dimmed={atMin} direction="left" nudge={false} size="inline" />
                </span>
              ) : null}
              <span className="zone-paused-control-text">
                <ZoneCaps fact>{shown}</ZoneCaps>
              </span>
              {interaction.kind === "range" ? (
                <span aria-hidden="true" className="zone-paused-control-meter" ref={meter}>
                  <i className="zone-paused-control-fill" ref={fill} />
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
