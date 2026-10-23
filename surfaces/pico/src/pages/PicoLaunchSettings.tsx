import "./PicoLaunchSettings.css"
import { type ComponentProps, useEffect, useLayoutEffect, useRef } from "react"
import { picoLaunchScope, picoLaunchShown, type PicoLaunchView } from "../pico-launch-view"
import { PicoButton } from "../ui/atoms/PicoButton"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoScopeTabs } from "../ui/molecules/PicoScopeTabs"
import { PicoSettings } from "./PicoSettings"

type SettingsProps = ComponentProps<typeof PicoSettings>

/**
 * A game's launch settings: Settings' own screen, one spine per layer, and
 * at the head of the layer's card one tab per runner scope (any runner, a
 * family, each runner). The scope chosen holds across spines, so moving from
 * Device to Game keeps showing mGBA's values.
 *
 * Every row is a Korri setting, so a press means what it means in Settings: a
 * percent or a governor steps to the next choice, a number or a profile's
 * name opens the keyboard, Add setting opens the list of the runner's
 * settings, Delete profile asks first in Korri's words. Each row says where
 * its value in force comes from and whether it stays on this device. Rows a
 * file sets (environment, arguments, hooks) are statements: the cursor
 * reaches them, a press does nothing.
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
  scope,
  onScope,
  onAction,
  ...settings
}: {
  readonly launch: PicoLaunchView
  /** The runner scope the layer's card shows, by its place in the layer. */
  readonly scope: number
  readonly onScope: (scope: number) => void
  readonly onAction: (actionId: string) => void
} & Omit<SettingsProps, "settings" | "title" | "notice" | "lead" | "wifi" | "onSubmitWifi" | "onToggleShown" | "panelHead">) {
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
  const banner = launch.notes.length === 0 && extra.length === 0 ? undefined : (
    <div className="pico-launch-settings-note">
      {launch.notes.map((note, index) => (
        // Notes may repeat; a note's place in Korri's list is its identity.
        <p key={`${index}:${note}`}>{note}</p>
      ))}
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
  const layer = launch.layers[Math.min(settings.group, launch.layers.length - 1)]
  const shownScope = picoLaunchScope(layer, scope)
  const head = layer === undefined || layer.scopes.length < 2 ? undefined : (
    <PicoScopeTabs current={shownScope} label="Runner" onSelect={onScope} scopes={layer.scopes} />
  )
  return (
    <div className="pico-launch-settings" ref={root}>
      <PicoSettings
        {...settings}
        banner={banner}
        {...(head === undefined ? {} : { panelHead: head })}
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
        settings={picoLaunchShown(launch, shownScope)}
        title="LAUNCH SETTINGS"
        {...(title === undefined ? {} : { from: title })}
      />
    </div>
  )
}
