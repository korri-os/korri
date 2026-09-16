import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { useEffect, useLayoutEffect, useRef } from "react"
import { PicoButton } from "../atoms/PicoButton"
import { PicoGameOverlay } from "../templates/PicoGameOverlay"

/** Shape: runner facts use Pico's scrollable panel and native buttons. */
export function PicoRunnerPicker({
  choice,
  onAction,
}: {
  readonly choice: Exclude<SurfaceRunnerChoice, { _tag: "Closed" }>
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
    <div className="pico-runner-picker lrud-container" data-block-exit="true" ref={root}>
      <PicoGameOverlay
        label={`Runners for ${choice.gameTitle}`}
        hints={[
          { hintKey: "a", label: "SELECT" },
          { hintKey: "b", label: "CANCEL" },
        ]}
      >
        <h1>RUNTIMES · {choice.gameTitle}</h1>
        <p role="status">{choice.message}</p>
        {choice.saved.map(saved => (
          <p key={saved.label}>
            {saved.label}: {saved.runnerId}
          </p>
        ))}
        {[...new Set(choice.warnings)].map(warning => (
          <p key={warning}>{warning}</p>
        ))}
        {choice.routes.map(route => (
          <section className="pico-runner-picker-route" key={route.runnerId}>
            <h2>{route.runnerId}</h2>
            <dl>
              <dt>System</dt>
              <dd>{route.systemId}</dd>
              <dt>Family</dt>
              <dd>{route.familyId ?? "None"}</dd>
              <dt>Runner build</dt>
              <dd>{route.runnerBuild}</dd>
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
