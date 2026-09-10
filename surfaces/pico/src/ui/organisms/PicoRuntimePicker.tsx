import type { SurfaceRuntimeChoice } from "@contracts/surface/korri-surface"
import { useEffect, useLayoutEffect, useRef } from "react"
import { PicoButton } from "../atoms/PicoButton"
import { PicoGameOverlay } from "../templates/PicoGameOverlay"

/** Shape: runtime facts use Pico's scrollable panel and native buttons. */
export function PicoRuntimePicker({
  choice,
  onAction,
}: {
  readonly choice: Exclude<SurfaceRuntimeChoice, { _tag: "Closed" }>
  readonly onAction: (id: string) => void
}) {
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: A new route state replaces the focusable controls.
  useEffect(() => {
    root.current?.querySelector<HTMLButtonElement>("button")?.focus()
  }, [choice._tag])
  return (
    <div className="pico-runtime-picker lrud-container" data-block-exit="true" ref={root}>
      <PicoGameOverlay
        label={`Runtimes for ${choice.gameTitle}`}
        hints={[
          { hintKey: "a", label: "SELECT" },
          { hintKey: "b", label: "CANCEL" },
        ]}
      >
        <h1>RUNTIMES · {choice.gameTitle}</h1>
        <p role="status">{choice.message}</p>
        {choice.saved.map(saved => (
          <p key={saved.label}>
            {saved.label}: {saved.runtimeId}
          </p>
        ))}
        {[...new Set(choice.warnings)].map(warning => (
          <p key={warning}>{warning}</p>
        ))}
        {choice.routes.map(route => (
          <section className="pico-runtime-picker-route" key={route.runtimeId}>
            <h2>{route.runtimeId}</h2>
            <dl>
              <dt>System</dt>
              <dd>{route.systemId}</dd>
              <dt>Launcher</dt>
              <dd>{route.launcherId}</dd>
              <dt>Launcher kind</dt>
              <dd>{route.launcherKind}</dd>
              <dt>Runtime build</dt>
              <dd>{route.runtimeBuild}</dd>
              <dt>Launcher build</dt>
              <dd>{route.launcherBuild}</dd>
              <dt>Program</dt>
              <dd>{route.program}</dd>
            </dl>
            {[...new Set(route.warnings)].map(warning => (
              <p key={warning}>{warning}</p>
            ))}
            {route.actions.map(action => (
              <PicoButton
                key={action.id}
                label={action.label}
                onPress={() => {
                  if (action.enabled) onAction(action.id)
                }}
              />
            ))}
          </section>
        ))}
        {choice.actions.map(action => (
          <PicoButton
            key={action.id}
            label={action.label}
            onPress={() => {
              if (action.enabled) onAction(action.id)
            }}
          />
        ))}
      </PicoGameOverlay>
    </div>
  )
}
