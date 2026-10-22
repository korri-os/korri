import "./PicoLaunchSettings.css"
import { type ComponentProps, useEffect, useLayoutEffect, useRef } from "react"
import type { PicoLaunchView } from "../pico-launch-view"
import { PicoButton } from "../ui/atoms/PicoButton"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoSettings } from "./PicoSettings"

type SettingsProps = ComponentProps<typeof PicoSettings>

/**
 * A game's launch settings: Settings' own screen, one spine per layer.
 *
 * Every row is a Korri setting, so a press means what it means in Settings: a
 * percent or a governor steps to the next choice, a profile's name opens the
 * keyboard, Delete profile asks first in Korri's words. Each row says where
 * its value in force comes from and that it stays on this device.
 *
 * While Korri reads, or after it reports a change made elsewhere or a
 * failure, a notice takes the rows' place with exactly the actions Korri
 * published. RELOAD stands on the floor row while the rows show. Back closes
 * a question, then the editor.
 *
 * The cursor is DOM mechanics, not state: it starts on the first row, comes
 * back to a row when what it was on went away (a notice that ended, a
 * profile that was deleted), and returns to the action that opened the
 * editor when it closes.
 */
export function PicoLaunchSettings({
  launch,
  onAction,
  ...settings
}: {
  readonly launch: PicoLaunchView
  readonly onAction: (actionId: string) => void
} & Omit<SettingsProps, "settings" | "title" | "notice" | "lead" | "wifi" | "onSubmitWifi" | "onToggleShown">) {
  const root = useRef<HTMLDivElement>(null)
  const opener = useRef(document.activeElement)
  useLayoutEffect(() => {
    const previous = opener.current
    return () => {
      queueMicrotask(() => {
        if (previous instanceof HTMLElement && previous.isConnected) previous.focus()
      })
    }
  }, [])
  useEffect(() => {
    const node = root.current
    if (node === null) return
    const active = node.ownerDocument.activeElement
    if (active !== null && active !== node.ownerDocument.body && active.isConnected && node.contains(active)) return
    const target = node.querySelector<HTMLElement>(".pico-setting-row button:not([disabled])")
      ?? node.querySelector<HTMLElement>("button:not([disabled])")
    target?.focus()
  })
  const actions = launch.actions.map(action => ({
    label: action.label.toUpperCase(),
    disabled: !action.enabled,
    onPress: () => onAction(action.id),
  }))
  const title = launch.gameTitle
  const notice = launch.phase === "Loading" ? (
    <PicoNotice actions={actions} kicker="READING LAUNCH SETTINGS" title={title} tone="info" />
  ) : launch.phase === "Conflict" ? (
    <PicoNotice actions={actions} kicker="CHANGED ELSEWHERE" message={launch.message} title={title} tone="ask" />
  ) : launch.phase === "Error" ? (
    <PicoNotice actions={actions} kicker="LAUNCH SETTINGS FAILED" message={launch.message} title={title} tone="warn" />
  ) : undefined
  // An inert action stays on screen, dimmed (the treaty's "present but inert").
  const reload = launch.phase === "Ready"
    ? launch.actions.find(action => action.id.endsWith(":reload"))
    : undefined
  // Korri's other actions (Choose a runner) belong to the note that explains them.
  const extra = launch.actions.filter(action => action.id !== launch.closeActionId && action !== reload)
  const banner = launch.note === undefined ? undefined : (
    <div className="pico-launch-settings-note">
      <p>{launch.note}</p>
      {extra.map(action => (
        <PicoButton
          disabled={!action.enabled}
          key={action.id}
          label={action.label.toUpperCase()}
          onPress={() => onAction(action.id)}
          tone="quiet"
        />
      ))}
    </div>
  )
  return (
    <div className="pico-launch-settings" ref={root}>
      <PicoSettings
        {...settings}
        banner={banner}
        lead={reload === undefined ? undefined : (
          <PicoButton
            disabled={!reload.enabled}
            label={reload.label.toUpperCase()}
            onPress={() => onAction(reload.id)}
            tone="quiet"
          />
        )}
        notice={notice}
        onSubmitWifi={() => undefined}
        onToggleShown={() => undefined}
        settings={launch.settings}
        title="LAUNCH SETTINGS"
        {...(title === undefined ? {} : { from: title })}
      />
    </div>
  )
}
