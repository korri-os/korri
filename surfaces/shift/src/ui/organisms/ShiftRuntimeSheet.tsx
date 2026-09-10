import type { SurfaceRuntimeChoice } from "@contracts/surface/korri-surface"
import { useEffect, useLayoutEffect, useRef } from "react"
import { ShiftSheetAction } from "../molecules/ShiftSheetAction"
import { ShiftSheetBody } from "./ShiftSheetBody"
import { ShiftSheetGroup } from "./ShiftSheetGroup"
import { ShiftSheetHeader } from "./ShiftSheetHeader"
import { ShiftSheetPanel } from "./ShiftSheetPanel"
import { ShiftSheetRoot } from "./ShiftSheetRoot"
import { ShiftSheetTitle } from "./ShiftSheetTitle"

/** Shape: installed-runtime facts composed with Shift's existing sheet system. */
export function ShiftRuntimeSheet({
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
    root.current
      ?.querySelector<HTMLButtonElement>(".shift-sheet-body button:not([disabled])")
      ?.focus()
  }, [choice._tag])
  return (
    <div ref={root} className="shift-runtime-sheet">
      <ShiftSheetRoot
        open
        label={`Runtimes for ${choice.gameTitle}`}
        onClose={() => onAction("runtime:cancel")}
      >
        <ShiftSheetPanel>
          <ShiftSheetHeader>
            <ShiftSheetTitle>Runtimes · {choice.gameTitle}</ShiftSheetTitle>
          </ShiftSheetHeader>
          <ShiftSheetBody>
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
              <ShiftSheetGroup key={route.runtimeId} title={route.runtimeId}>
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
                  <ShiftSheetAction
                    key={action.id}
                    label={action.label}
                    disabled={!action.enabled}
                    onSelect={() => onAction(action.id)}
                  />
                ))}
              </ShiftSheetGroup>
            ))}
            <ShiftSheetGroup title="Choices">
              {choice.actions.map(action => (
                <ShiftSheetAction
                  key={action.id}
                  label={action.label}
                  disabled={!action.enabled}
                  onSelect={() => onAction(action.id)}
                />
              ))}
            </ShiftSheetGroup>
          </ShiftSheetBody>
        </ShiftSheetPanel>
      </ShiftSheetRoot>
    </div>
  )
}
