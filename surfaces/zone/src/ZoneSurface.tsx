import "./ZoneSurface.css"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useMemo, useRef, type ReactNode } from "react"
import { ZoneGames } from "./pages/ZoneGames"
import { ZoneOnboarding } from "./pages/ZoneOnboarding"
import { ZoneOptions } from "./pages/ZoneOptions"
import { ZonePaused } from "./pages/ZonePaused"
import { ZoneRunWith } from "./pages/ZoneRunWith"
import { ZoneSaveData } from "./pages/ZoneSaveData"
import { catalogGames } from "./state/games"
import { ZONE_TABS, type ZoneTab } from "./state/navigation"
import { canGoOn, deviceNameSetting, onboardingJoin } from "./state/onboarding"
import { zoneShown, type ZoneShown, type ZoneSurfaceMessage } from "./state/surface"
import { ZoneKeycap } from "./ui/atoms/ZoneKeycap"
import { ZoneFoot } from "./ui/molecules/ZoneFoot"
import { ZoneTabStrip, type ZoneTabItem } from "./ui/molecules/ZoneTabStrip"
import { ZoneGround } from "./ui/organisms/ZoneGround"
import { ZoneStatusLayer } from "./ui/organisms/ZoneStatusLayer"
import { useZoneSurface, zoneDeviceName, zoneGamesPage, type ZoneSurfaceProgram } from "./use-zone-games"

/** The tabs as every page names them. */
const TABS: readonly ZoneTabItem[] = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]

const asTab = (id: string): ZoneTab | undefined => ZONE_TABS.find(tab => tab === id)

/** Pages that draw the ground themselves, behind their own picture. */
const OWN_GROUND: readonly ZoneShown[] = ["paused", "onboarding"]

/** Each model's dispatcher, built once per program. */
function senders(dispatch: (message: ZoneSurfaceMessage) => void) {
  return {
    nav: (message: Extract<ZoneSurfaceMessage, { _tag: "Games" }>["message"]) => dispatch({ _tag: "Games", message }),
    options: (message: Extract<ZoneSurfaceMessage, { _tag: "Options" }>["message"]) => dispatch({ _tag: "Options", message }),
    runWith: (message: Extract<ZoneSurfaceMessage, { _tag: "RunWith" }>["message"]) => dispatch({ _tag: "RunWith", message }),
    saveData: (message: Extract<ZoneSurfaceMessage, { _tag: "SaveData" }>["message"]) => dispatch({ _tag: "SaveData", message }),
    status: (message: Extract<ZoneSurfaceMessage, { _tag: "Status" }>["message"]) => dispatch({ _tag: "Status", message }),
    paused: (message: Extract<ZoneSurfaceMessage, { _tag: "Paused" }>["message"]) => dispatch({ _tag: "Paused", message }),
    onboarding: (message: Extract<ZoneSurfaceMessage, { _tag: "Onboarding" }>["message"]) => dispatch({ _tag: "Onboarding", message }),
    tab: (id: string) => {
      const tab = asTab(id)
      if (tab !== undefined) dispatch({ _tag: "Games", message: { _tag: "ChoseTab", tab } })
    },
    tabFocus: (id: string) => {
      const tab = asTab(id)
      if (tab !== undefined) dispatch({ _tag: "Games", message: { _tag: "FocusedTab", tab } })
    },
  }
}
type Senders = ReturnType<typeof senders>

/**
 * The composition root: the only file below the host door that reads the
 * treaty's model. It runs Zone's one program (useZoneSurface: every model,
 * the host's buttons, idle) and picks what takes the screen:
 *
 *     PAUSED      while Korri presents the gameplay overlay
 *     first boot  while Korri asks for it; OPTIONS' join keyboard over it
 *                 when a secured network needs its password
 *     RUN WITH    while Korri's runner choice is open
 *     the tab     GAMES (home), OPTIONS or SAVE DATA
 *
 * with the status layer over whichever it is. Each page receives plain
 * props and reports in its own model's messages; the program routes them.
 *
 * The one DOM mechanic here: on OPTIONS and SAVE DATA, which do not move
 * focus to the tabs themselves, the cursor goes to the tab navigation names
 * when it is on the tabs (a tab chosen, `menu`).
 */
export function ZoneSurface({ model, host }: { readonly model: SurfaceModel; readonly host: SurfaceHost }) {
  const root = useRef<HTMLDivElement | null>(null)
  const program = useZoneSurface(model, host, root)
  const { state, dispatch } = program
  const send = useMemo(() => senders(dispatch), [dispatch])
  const shown = zoneShown(state, model)
  const nav = state.nav

  const tabsHeld = (shown === "options" || shown === "savedata") && nav.zone === "tabs"
  const tabKey = tabsHeld ? `${shown}:${nav.tabCursor}` : ""
  useLayoutEffect(() => {
    if (!tabsHeld || root.current === null) return
    const at = TABS.findIndex(tab => tab.id === nav.tabCursor)
    const target = root.current.querySelectorAll<HTMLButtonElement>("[role='tablist'] button")[at]
    if (target === undefined || document.activeElement === target || target.closest("[inert]") !== null) return
    target.focus({ preventScroll: true })
    // tabKey names everything the effect reads.
  }, [tabKey])

  const titleOf = (gameId: string) => catalogGames(model.catalog).find(game => game.id === gameId)?.title

  return (
    <div className="zone-theme zone-screen" data-layer={nav.layers.at(-1) ?? "none"} data-shown={shown} ref={root}>
      {OWN_GROUND.includes(shown) ? null : <ZoneGround />}
      <ZoneStatusLayer
        focus={state.status.focus}
        onBusyAction={actionId => send.status({ _tag: "ChoseBusyAction", actionId })}
        onConfirmAction={actionId => send.status({ _tag: "ChoseConfirmAction", actionId })}
        onFocusButton={index => send.status({ _tag: "FocusedStatusButton", index })}
        onProblemButton={button => send.status({ _tag: "ChoseProblemButton", button })}
        page={zonePage(shown, program, model, host, send)}
        status={model.status}
        titleOf={titleOf}
      />
    </div>
  )
}

/** The page for what is shown, with plain props and its model's callbacks. */
function zonePage(
  shown: ZoneShown,
  program: ZoneSurfaceProgram,
  model: SurfaceModel,
  host: SurfaceHost,
  send: Senders,
): ReactNode {
  const { state, layout } = program
  const nav = state.nav
  const device = zoneDeviceName(model)
  const tabFocused = nav.zone === "tabs" ? nav.tabCursor : undefined
  switch (shown) {
    case "games":
      return <ZoneGames {...zoneGamesPage(program, model, host)} />
    case "options":
    case "join":
      return (
        <ZoneOptions
          band={layout.band}
          build={model.buildLabel}
          clock={model.clockLabel}
          device={device}
          korri={model}
          onClear={() => send.options({ _tag: "PressedClear" })}
          onDialogFocus={index => send.options({ _tag: "FocusedDialogButton", index })}
          onDialogPress={index => send.options({ _tag: "PressedDialogButton", index })}
          onKey={key => send.options({ _tag: "PressedKey", key })}
          onKeyFocus={keyId => send.options({ _tag: "FocusedKey", keyId })}
          onRowFocus={rowId => send.options({ _tag: "FocusedRow", rowId })}
          onRowPress={rowId => send.options({ _tag: "PressedRow", rowId })}
          onRowStep={(rowId, direction) => send.options({ _tag: "SteppedRow", rowId, direction })}
          onTab={send.tab}
          onTabFocus={send.tabFocus}
          strips={layout.strips}
          tabFocused={shown === "join" ? undefined : tabFocused}
          view={state.options}
        />
      )
    case "savedata":
      return (
        <ZoneSaveData
          card={state.identity.card}
          flow={state.identity.flow}
          frame={{
            band: layout.band,
            strips: layout.strips,
            hintBack: <ZoneKeycap button="B" word="Back" />,
            hintConfirm: <ZoneKeycap button="A" word="Confirm" />,
            tabs: (
              <ZoneTabStrip
                active="savedata"
                focused={tabFocused}
                onFocus={send.tabFocus}
                onSelect={send.tab}
                tabs={TABS}
              />
            ),
            foot: (
              <ZoneFoot
                build={model.buildLabel}
                clock={model.clockLabel}
                connection={model.network?.connection}
                device={device}
                strips={layout.strips}
              />
            ),
          }}
          identity={model.identityManagement}
          onEvent={send.saveData}
        />
      )
    case "runwith":
      return model.runnerChoice === undefined ? null : (
        <ZoneRunWith
          band={layout.band}
          build={model.buildLabel}
          choice={model.runnerChoice}
          clock={model.clockLabel}
          focused={state.runWith.focused}
          onAction={actionId => send.runWith({ _tag: "PressedAction", actionId })}
          onFocus={rowId => send.runWith({ _tag: "FocusedRow", rowId })}
          strips={layout.strips}
        />
      )
    case "paused":
      // PAUSED stands over a game that is still there: a dialog named for it.
      return model.presentation.kind !== "gameplay-overlay" ? null : (
        <div aria-label={model.presentation.title ?? "Paused"} aria-modal="true" className="zone-surface-paused" role="dialog">
          <ZonePaused
            answer={state.overlay.answer}
            asking={state.overlay.asking}
            focusedId={state.overlay.focusedId}
            moving={state.overlay.moving}
            onAnswer={yes => send.paused({ _tag: "ChoseAnswer", yes })}
            onFocusAnswer={index => send.paused({ _tag: "FocusedAnswer", index })}
            onFocusControl={controlId => send.paused({ _tag: "FocusedControl", controlId })}
            onPressControl={controlId => send.paused({ _tag: "PressedControl", controlId })}
            onStepControl={(controlId, direction) => send.paused({ _tag: "SteppedControl", controlId, direction })}
            overlay={model.presentation}
          />
        </div>
      )
    case "onboarding": {
      const onboarding = state.onboarding
      return (
        <ZoneOnboarding
          band={layout.band}
          canGoOn={canGoOn(onboarding.step, model)}
          controllers={model.onboarding?.controllers ?? []}
          entry={onboarding.entry}
          finishing={onboarding.finishing}
          focus={onboarding.focus}
          nameSetting={deviceNameSetting(model)}
          network={model.network}
          onFinish={() => send.onboarding({ _tag: "ChoseFinish" })}
          onFocusKey={id => send.onboarding({ _tag: "FocusedKey", id })}
          onFocusTarget={target => send.onboarding({ _tag: "FocusedTarget", target })}
          onJoin={networkId => send.onboarding(onboardingJoin(networkId))}
          onKey={key => send.onboarding({ _tag: "PressedKey", key })}
          onNext={() => send.onboarding({ _tag: "ChoseNext" })}
          onOpenName={() => send.onboarding({ _tag: "OpenedName" })}
          onScan={() => send.onboarding({ _tag: "ChoseScan" })}
          onSkip={() => send.onboarding({ _tag: "ChoseSkip" })}
          onStart={() => send.onboarding({ _tag: "ChoseStart" })}
          settingsStatus={model.settingsStatus}
          started={onboarding.started}
          step={onboarding.step}
          strips={layout.strips}
        />
      )
    }
  }
}
