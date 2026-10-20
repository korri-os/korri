import "./PicoStoreProviders.css"
import type { SurfaceStoreProviders } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { PicoRow } from "../atoms/PicoRow"
import { PicoNotice } from "../molecules/PicoNotice"

/**
 * Where games can come from: the providers Korri lists, one row each.
 *
 * While Korri reads them a blue card says so; a refusal is the red card with
 * Korri's words and its Retry; no provider at all says where to turn one on.
 * The cursor lands on the first row, so one press picks a provider.
 */
export function PicoStoreProviders({
  providers,
  onChoose,
  onAction,
}: {
  readonly providers: SurfaceStoreProviders
  readonly onChoose: (providerId: string) => void
  readonly onAction: (actionId: string) => void
}) {
  const root = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const scope = root.current
    if (scope === null) return
    const active = scope.ownerDocument.activeElement
    if (active !== null && active !== scope.ownerDocument.body && active.isConnected && !active.matches(":disabled")) return
    scope.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus()
  })
  return (
    <div className="pico-store-providers" ref={root}>
      {providers._tag === "Loading" ? (
        <PicoNotice kicker="LOOKING FOR PROVIDERS" message="Korri is listing where games can come from." tone="info" />
      ) : providers._tag === "Problem" ? (
        <PicoNotice
          actions={providers.actions.map(action => ({ label: action.label, disabled: !action.enabled, onPress: () => onAction(action.id) }))}
          kicker="NO PROVIDERS"
          message={providers.message}
          tone="warn"
        />
      ) : providers.providers.length === 0 ? (
        <PicoNotice
          kicker="NO PROVIDERS"
          message="No provider is turned on for this device. Turn one on in Settings to get games here."
          tone="info"
        />
      ) : (
        <>
          <p className="pico-store-providers-lead">Pick where to look for a game.</p>
          <ul className="pico-store-providers-list">
            {providers.providers.map(provider => (
              <li className="pico-store-providers-item" key={provider.id}>
                <PicoRow detail="▶" label={provider.title} onPress={() => onChoose(provider.id)} />
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
