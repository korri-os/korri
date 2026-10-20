import "./ZoneOnboarding.css"
import "../zone-motion.css"
import type {
  SurfaceController,
  SurfaceNetwork,
  SurfaceSettingItem,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { planControllerCards } from "../plan/onboarding"
import { useZoneLayerFocus } from "../use-zone-layer-focus"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import type { ZoneTagItem } from "../ui/atoms/ZoneTag"
import { ZoneActionRow } from "../ui/molecules/ZoneActionRow"
import { ZoneKeyboard, type ZoneKey, type ZoneKeyboardLayer } from "../ui/molecules/ZoneKeyboard"
import { ZoneOptionGroup } from "../ui/molecules/ZoneOptionGroup"
import { ZoneSaveCard } from "../ui/molecules/ZoneSaveCard"
import { ZoneTagRow } from "../ui/molecules/ZoneTagRow"
import { ZoneControllerCard } from "../ui/organisms/ZoneControllerCard"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"

export type ZoneOnboardingStepId = "controllers" | "wifi" | "name" | "identity"

/** The keyboard over the NAME step. */
export interface ZoneOnboardingEntry {
  readonly draft: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey: string
}

/**
 * The focus targets this page draws, by id. The onboarding model names its
 * cursor with the same ids (src/state/onboarding.ts `TARGET`), and
 * test/zone-onboarding.test.tsx holds the two equal.
 */
export const ONBOARDING_TARGET = {
  next: "next",
  finish: "finish",
  save: "save",
  connection: "connection",
  scan: "scan",
  skip: "skip",
  name: "name",
} as const

export const onboardingNetworkTarget = (networkId: string): string => `net:${networkId}`

const STEPS: readonly { readonly id: ZoneOnboardingStepId; readonly label: string }[] = [
  { id: "controllers", label: "1 Controllers" },
  { id: "wifi", label: "2 Wi-Fi" },
  { id: "name", label: "3 Name" },
  { id: "identity", label: "4 Identity" },
]

/** The step strip: done steps blue, this one yellow, the rest white. */
function stepTags(step: ZoneOnboardingStepId): readonly ZoneTagItem[] {
  const at = STEPS.findIndex(item => item.id === step)
  return STEPS.map((item, index) => ({
    kind: index < at ? "system" : index === at ? "here" : "continue",
    label: item.label,
  }))
}

function connectionText(network: SurfaceNetwork): string {
  switch (network.connection._tag) {
    case "Offline":
      return "Offline"
    case "Wired":
      return "Wired"
    case "Wifi":
      return network.connection.ssid
  }
}

/** The WI-FI step's rows: the connection, each network, SCAN and SKIP. */
function wifiRows(network: SurfaceNetwork | undefined): readonly SurfaceSettingItem[] {
  const wifi = network?.wifi
  return [
    ...(network === undefined ? [] : [{ id: ONBOARDING_TARGET.connection, label: "Connection", value: connectionText(network) }]),
    ...(wifi?._tag === "Available" ? wifi.networks.map(item => ({
      id: onboardingNetworkTarget(item.id),
      label: item.ssid,
      value: `${item.strengthPercent}% ${item.known ? "Saved" : item.secured ? "Locked" : "Open"}`,
      interaction: { kind: "action", actionId: item.id } as const,
    })) : []),
    ...(wifi?._tag === "Available"
      ? [{ id: ONBOARDING_TARGET.scan, label: "Networks", value: "Scan", interaction: { kind: "action", actionId: "scan" } as const }]
      : []),
    { id: ONBOARDING_TARGET.skip, label: "Skip", value: "Later", interaction: { kind: "action", actionId: "skip" } },
  ]
}

/** Korri's attempt and scan as the rows' saving and problem marks. */
function wifiStatus(network: SurfaceNetwork | undefined): SurfaceSettingsStatus {
  const wifi = network?.wifi
  if (wifi?._tag !== "Available") return { _tag: "Idle" }
  const attempt = wifi.attempt
  const rowOf = (ssid: string) => {
    const item = wifi.networks.find(candidate => candidate.ssid === ssid)
    return item === undefined ? undefined : onboardingNetworkTarget(item.id)
  }
  if (attempt._tag === "Connecting") {
    const row = rowOf(attempt.ssid)
    if (row !== undefined) return { _tag: "Saving", settingId: row }
  }
  if (attempt._tag === "Failed") {
    const row = rowOf(attempt.ssid)
    if (row !== undefined) return { _tag: "Problem", settingId: row, message: attempt.message }
  }
  return wifi.scanning ? { _tag: "Saving", settingId: ONBOARDING_TARGET.scan } : { _tag: "Idle" }
}

/**
 * First boot (C25), in four steps under a strip that shows where you are.
 *
 *  1. CONTROLLERS: COMPETITION's row of cards, one per controller Korri
 *     sees, WAITING until it is heard, then its name and seat. NEXT lights
 *     once any is heard.
 *  2. WI-FI: an OPTIONS group: the connection, each network Korri found
 *     (A joins), SCAN, and SKIP. NEXT lights once the device is online.
 *  3. NAME: the DEVICE NAME row; A raises the keyboard, DONE saves it.
 *  4. IDENTITY: the NEW SAVE card; START chooses it, and FINISH ends setup.
 *
 * B goes back a step (the binding routes it). Every fact is Korri's; the
 * step, the cursor and the draft are the onboarding model's, as props.
 */
export function ZoneOnboarding({
  step,
  focus,
  entry,
  started = false,
  finishing = false,
  canGoOn = false,
  controllers,
  network,
  nameSetting,
  settingsStatus = { _tag: "Idle" },
  strips,
  band,
  onFocusTarget,
  onNext,
  onSkip,
  onScan,
  onJoin,
  onOpenName,
  onKey,
  onStart,
  onFinish,
}: {
  readonly step: ZoneOnboardingStepId
  /** The target the cursor is on, as ONBOARDING_TARGET names it. */
  readonly focus: string
  readonly entry?: ZoneOnboardingEntry
  readonly started?: boolean
  readonly finishing?: boolean
  /** Whether NEXT leads on from this step. */
  readonly canGoOn?: boolean
  readonly controllers: readonly SurfaceController[]
  readonly network?: SurfaceNetwork
  /** Korri's device-name setting, as the binding found it; absent, the row is read-only. */
  readonly nameSetting?: SurfaceSettingItem
  readonly settingsStatus?: SurfaceSettingsStatus
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly onFocusTarget?: (target: string) => void
  readonly onNext?: () => void
  readonly onSkip?: () => void
  readonly onScan?: () => void
  readonly onJoin?: (networkId: string) => void
  readonly onOpenName?: () => void
  readonly onKey?: (key: ZoneKey) => void
  readonly onStart?: () => void
  readonly onFinish?: () => void
}) {
  const body = useRef<HTMLDivElement | null>(null)
  const stage = useRef<HTMLDivElement | null>(null)
  const cards = useRef<HTMLDivElement | null>(null)
  const keyboard = useRef<HTMLDivElement | null>(null)
  useZoneLayerFocus(body, entry === undefined ? `${step}:${focus}` : undefined)
  useZoneLayerFocus(keyboard, entry?.focusedKey)

  // The cards' grid follows the stage's size: a measurement written to the
  // DOM, as the band writes its focus, not state.
  const cardCount = Math.max(1, controllers.length)
  useLayoutEffect(() => {
    const room = stage.current
    const grid = cards.current
    if (room === null || grid === null) return
    const apply = () => {
      const layout = planControllerCards(room.clientWidth, room.clientHeight, cardCount)
      grid.style.setProperty("--zone-onboarding-card-w", `${layout.cardWidth}px`)
      grid.style.setProperty("--zone-onboarding-columns", String(layout.columns))
    }
    apply()
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(apply)
    observer?.observe(room)
    return () => observer?.disconnect()
  }, [cardCount, step])

  const heardOrder = controllers.filter(controller => controller.heard).map(controller => controller.id)
  const nameRow: SurfaceSettingItem = nameSetting === undefined
    ? { id: ONBOARDING_TARGET.name, label: "Device name" }
    : { ...nameSetting, id: ONBOARDING_TARGET.name }
  const nameStatus: SurfaceSettingsStatus = settingsStatus._tag !== "Idle" && settingsStatus.settingId === nameSetting?.id
    ? { ...settingsStatus, settingId: ONBOARDING_TARGET.name } : { _tag: "Idle" }
  const maxLength = nameSetting?.interaction?.kind === "text" ? nameSetting.interaction.maxLength : undefined
  const forward = step === "identity"
    ? { id: ONBOARDING_TARGET.finish, label: "Finish", disabled: !started }
    : { id: ONBOARDING_TARGET.next, label: "Next", disabled: !canGoOn }

  const onRow = (target: string) => {
    if (target === ONBOARDING_TARGET.skip) onSkip?.()
    else if (target === ONBOARDING_TARGET.scan) onScan?.()
    else if (target === ONBOARDING_TARGET.name) onOpenName?.()
    else if (target.startsWith("net:")) onJoin?.(target.slice("net:".length))
  }

  return (
    <div className="zone-onboarding" data-step={step}>
      <ZoneGround />
      <ZoneShell
        band={band}
        hintBack={<ZoneKeycap button="B" word="Back" />}
        hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
        strips={strips}
        tabs={(
          <span aria-label="Setup steps" className="zone-onboarding-steps" role="group">
            <ZoneTagRow tags={stepTags(step)} />
          </span>
        )}
      >
        <div className="zone-onboarding-body" inert={entry !== undefined} ref={body}>
          <div className="zone-onboarding-stage" ref={stage}>
            {step === "controllers" ? (
              <div className="zone-onboarding-cards" ref={cards}>
                {controllers.length === 0 ? <ZoneControllerCard /> : controllers.map(controller => {
                  const seat = heardOrder.indexOf(controller.id)
                  return controller.heard
                    ? <ZoneControllerCard key={controller.id} name={controller.name} seat={seat + 1} />
                    : <ZoneControllerCard key={controller.id} />
                })}
              </div>
            ) : null}
            {step === "wifi" ? (
              <div className="zone-onboarding-group">
                <ZoneOptionGroup
                  focusedId={focus}
                  onFocus={onFocusTarget}
                  onPress={onRow}
                  rows={wifiRows(network)}
                  status={wifiStatus(network)}
                  title="Wi-Fi"
                />
              </div>
            ) : null}
            {step === "name" ? (
              <div className="zone-onboarding-group">
                <ZoneOptionGroup
                  focusedId={focus}
                  onFocus={onFocusTarget}
                  onPress={onRow}
                  rows={[nameRow]}
                  status={nameStatus}
                  title="Device"
                />
              </div>
            ) : null}
            {step === "identity" ? (
              <span className="zone-onboarding-save" onClick={started || finishing ? undefined : onStart}>
                <ZoneSaveCard
                  body={{ kind: "text", text: "Start" }}
                  focused={focus === ONBOARDING_TARGET.save}
                  header="New save"
                  onFocus={onFocusTarget === undefined ? undefined : () => onFocusTarget(ONBOARDING_TARGET.save)}
                  tag={started ? { kind: "saved", label: "Ready" } : undefined}
                />
              </span>
            ) : null}
          </div>
          <div className="zone-onboarding-actions">
            <ZoneActionRow
              actions={[forward]}
              focused={focus === forward.id ? forward.id : undefined}
              lit={forward.disabled ? undefined : forward.id}
              onFocus={onFocusTarget}
              onPress={() => {
                if (forward.disabled || finishing) return
                if (step === "identity") onFinish?.()
                else onNext?.()
              }}
              pressed={finishing ? ONBOARDING_TARGET.finish : undefined}
            />
          </div>
        </div>
        {entry === undefined ? null : (
          <div className="zone-onboarding-keyboard" data-block-exit ref={keyboard}>
            <ZoneKeyboard
              caret={entry.caret}
              draft={entry.draft}
              focusedKey={entry.focusedKey}
              layer={entry.layer}
              maxLength={maxLength}
              onKey={onKey}
            />
          </div>
        )}
      </ZoneShell>
    </div>
  )
}
