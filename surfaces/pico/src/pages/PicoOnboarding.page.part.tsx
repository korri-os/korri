import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { useEffect, useState } from "react"
import { createFixtureHost } from "../fixtures/fixture-host"
import { useFixtureDpad } from "../fixtures/fixture-dpad"
import {
  answerRequest,
  emptyScanNetwork,
  fixtureStarterPackInstall,
  heardControllers,
  noAdapterNetwork,
  onboardingModel,
  starterPackFailed,
  starterPackFailedLong,
  starterPackModel,
  starterPackOffered,
  unreachableNetwork,
  withStarterPack,
  wiredNetwork,
} from "../fixtures/onboarding"
import { performPicoRequest } from "../pico-host"
import { picoOnboardingScreen, picoSetupCursor, picoSetupPlace, picoSetupTrack } from "../pico-onboarding-view"
import { initialOnboarding, type OnboardingMessage, type OnboardingNav } from "../state/onboarding"
import { updateOnboarding } from "../state/onboarding-update"
import type { PicoRequest } from "../state/requests"
import { usePicoProgram } from "../use-pico-program"
import { Ready as HomeReady } from "./PicoHome.page.part"
import { PicoOnboarding, type PicoOnboardingActions } from "./PicoOnboarding"

export const name = "Onboarding"
export const note = "First-boot setup, Hello to Play. Every state can be walked: arrows are the d-pad, Enter is A, Escape is B"

/**
 * A scripted start: a message; `answer`, where Korri publishes every answer
 * still due; or a change to Korri's model, as Korri would publish it.
 */
type Beat = OnboardingMessage | "answer" | { readonly change: (model: SurfaceModel) => SurfaceModel }

/* Run setup's real update against the fixture Korri (fixtures/onboarding.ts),
 * which answers each request at once and keeps its later answer for "answer". */
function play(start: SurfaceModel, beats: readonly Beat[]): { readonly nav: OnboardingNav; readonly korri: SurfaceModel } {
  let korri = start
  let nav = initialOnboarding()
  let due: ((model: SurfaceModel) => SurfaceModel)[] = []
  const run = (message: OnboardingMessage) => {
    const step = updateOnboarding(nav, message, korri)
    nav = step.model
    for (const request of step.requests) {
      const answer = answerRequest(request)
      if (answer === undefined) continue
      if (answer.later !== undefined) due.push(answer.later.then)
      publish(answer.now(korri))
    }
  }
  const publish = (next: SurfaceModel) => {
    korri = next
    run({ _tag: "KorriPublished" })
  }
  for (const beat of beats) {
    if (beat === "answer") {
      const now = due
      due = []
      for (const then of now) publish(then(korri))
    } else if ("change" in beat) {
      publish(beat.change(korri))
    } else {
      run(beat)
    }
  }
  return { nav, korri }
}

const typed = (text: string): readonly Beat[] => [...text].map(character => ({ _tag: "TypedText", character }))

/* The walk: setup's program against a fixture Korri that answers by itself,
 * started from a scripted point. */
function Walk({ start = onboardingModel, beats = [] }: { readonly start?: SurfaceModel; readonly beats?: readonly Beat[] }) {
  const [opening] = useState(() => play(start, beats))
  const [korri, setKorri] = useState(opening.korri)
  const [host] = useState(createFixtureHost)
  const perform = (request: PicoRequest) => {
    const answer = answerRequest(request)
    if (answer === undefined) return performPicoRequest(host, { chooseFont: () => undefined }, request)
    setKorri(answer.now)
    const later = answer.later
    if (later !== undefined) setTimeout(() => setKorri(later.then), later.ms)
    return undefined
  }
  const { model: nav, dispatch } = usePicoProgram<OnboardingNav, OnboardingMessage, SurfaceModel>(
    () => opening.nav, updateOnboarding, korri, perform)
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [korri, dispatch])
  useFixtureDpad({ onBack: () => dispatch({ _tag: "PressedBack" }) })

  if (korri.onboarding === undefined) return <HomeReady />
  const screen = picoOnboardingScreen(nav, korri)
  const seats = screen._tag === "Pads" ? screen.view.seats : undefined
  const actions: PicoOnboardingActions = {
    onStart: () => dispatch({ _tag: "Started" }),
    onContinue: () => dispatch({ _tag: "Continued" }),
    wifi: {
      onPick: networkId => dispatch({ _tag: "PickedNetwork", networkId }),
      onHidden: () => dispatch({ _tag: "ChoseHiddenNetwork" }),
      onScan: () => dispatch({ _tag: "ScannedNetworks" }),
      onContinue: () => dispatch({ _tag: "Continued" }),
      onTryAgain: () => dispatch({ _tag: "TriedAgain" }),
      onPickAnother: () => dispatch({ _tag: "PickedAnotherNetwork" }),
      onStay: () => dispatch({ _tag: "StayedConnected" }),
      onSetUpWifi: () => dispatch({ _tag: "ChoseWifiToo" }),
      onOffline: () => dispatch({ _tag: "ChoseOffline" }),
      onConfirmOffline: () => dispatch({ _tag: "ConfirmedOffline" }),
      onBackToWifi: () => dispatch({ _tag: "ReturnedToWifi" }),
    },
    typing: {
      onType: character => dispatch({ _tag: "TypedText", character }),
      onBackspace: () => dispatch({ _tag: "PressedTextBackspace" }),
      onClear: () => dispatch({ _tag: "ClearedText" }),
      onToggleCapitals: () => dispatch({ _tag: "ToggledCapitals" }),
      onToggleSymbols: () => dispatch({ _tag: "ToggledSymbols" }),
    },
    onToggleShown: () => dispatch({ _tag: "ToggledShown" }),
    onSubmitText: () => dispatch({ _tag: "SubmittedText" }),
    onDismissProblem: () => dispatch({ _tag: "DismissedSettingsProblem" }),
    identity: {
      onChoose: how => dispatch({ _tag: "ChoseIdentity", how }),
      onEdit: field => dispatch({ _tag: "EditedIdentityField", field }),
      onSubmit: () => dispatch({ _tag: "SubmittedIdentity" }),
      onBackUp: () => dispatch({ _tag: "OpenedBackup" }),
      onConfirm: () => dispatch({ _tag: "ToggledTrustLoss" }),
    },
    backup: {
      onEdit: password => dispatch({ _tag: "EditedBackupPassword", password }),
      onSubmit: () => dispatch({ _tag: "SubmittedBackup" }),
      onClose: () => dispatch({ _tag: "ClosedBackup" }),
    },
    onSeats: () => {
      if (seats?.control.kind === "step" || seats?.control.kind === "cycle") {
        dispatch({ _tag: "ChangedSetting", settingId: seats.id, value: seats.control.next })
      }
    },
    onScanGames: () => dispatch({ _tag: "ScannedGames" }),
    onStarterPack: actionId => dispatch({ _tag: "ChoseStarterPackAction", actionId }),
    onFinish: () => dispatch({ _tag: "Finished" }),
  }
  return (
    <PicoOnboarding
      actions={actions}
      cursor={picoSetupCursor(nav.step, screen)}
      place={picoSetupPlace(nav.step, screen)}
      screen={screen}
      track={picoSetupTrack(nav, korri)}
    />
  )
}

/* ---- Scripted starts ---- */

const started: readonly Beat[] = [{ _tag: "Started" }]
const scanned: readonly Beat[] = [...started, "answer"]
const homePassword: readonly Beat[] = [...scanned, { _tag: "PickedNetwork", networkId: "ap-home-5g" }, ...typed("pixel-Party8")]
const joining: readonly Beat[] = [...homePassword, { _tag: "SubmittedText" }]
const connected: readonly Beat[] = [...joining, "answer"]
const naming: readonly Beat[] = [...connected, { _tag: "Continued" }]
const named: readonly Beat[] = [...naming, ...typed("Pocket"), { _tag: "SubmittedText" }]
const choosing: readonly Beat[] = [...named, "answer"]
const created: readonly Beat[] = [...choosing, { _tag: "ChoseIdentity", how: "created" }]
const restoring: readonly Beat[] = [
  ...choosing,
  { _tag: "ChoseIdentity", how: "restored" },
  { _tag: "EditedIdentityField", field: "backup" },
  ...typed("ncryptsec1qgg9947rlpvqu76pj5ecreduyjmn"),
  { _tag: "SubmittedText" },
  { _tag: "EditedIdentityField", field: "password" },
  ...typed("preview-only"),
  { _tag: "SubmittedText" },
]
/* The player's own yes to the trust-loss question. */
const confirmed: Beat = { _tag: "ToggledTrustLoss" }
const signing: readonly Beat[] = [
  ...choosing,
  { _tag: "ChoseIdentity", how: "signer" },
  { _tag: "EditedIdentityField", field: "uri" },
  ...typed("bunker://7e1f0c3a9b?relay=wss://relay.example"),
  { _tag: "SubmittedText" },
]
const pads: readonly Beat[] = [...created, { _tag: "Continued" }]
const heard: Beat = { change: model => ({ ...model, onboarding: { ...model.onboarding, controllers: heardControllers } }) }
const padsReady: readonly Beat[] = [...pads, heard]
const games: readonly Beat[] = [...padsReady, { _tag: "Continued" }]
const done: readonly Beat[] = [...games, { _tag: "Continued" }]
/* The offline path: NETGEAR-7B21 refuses, the player goes on with no Wi-Fi. */
const refused: readonly Beat[] = [...scanned, { _tag: "PickedNetwork", networkId: "ap-netgear" }, ...typed("netgear123"), { _tag: "SubmittedText" }, "answer"]
const warned: readonly Beat[] = [...refused, { _tag: "ChoseOffline" }]
const offlineChoosing: readonly Beat[] = [...warned, { _tag: "ConfirmedOffline" }, ...typed("Pocket"), { _tag: "SubmittedText" }, "answer"]
const offlineDone: readonly Beat[] = [
  ...offlineChoosing, { _tag: "ChoseIdentity", how: "created" }, { _tag: "Continued" }, heard, { _tag: "Continued" }, { _tag: "Continued" },
]
const nothingFound: readonly Beat[] = [...started, { change: model => ({ ...model, network: emptyScanNetwork }) }]

export default function PicoOnboardingWalk() {
  return <Walk />
}

export function HelloOnCable() {
  return <Walk start={{ ...onboardingModel, network: wiredNetwork }} />
}

export function WifiScanning() {
  return <Walk beats={started} />
}

export function WifiNetworks() {
  return <Walk beats={scanned} />
}

export function WifiPassword() {
  return <Walk beats={homePassword} />
}

export function WifiPasswordShown() {
  return <Walk beats={[...homePassword, { _tag: "ToggledShown" }]} />
}

export function WifiConnecting() {
  return <Walk beats={joining} />
}

export function WifiConnected() {
  return <Walk beats={connected} />
}

export function WifiWrongPassword() {
  return <Walk beats={refused} />
}

export function WifiCouldNotConnect() {
  return <Walk beats={scanned} start={{ ...onboardingModel, network: unreachableNetwork }} />
}

export function WifiOfflineWarning() {
  return <Walk beats={warned} />
}

export function WifiNoNetworks() {
  return <Walk beats={nothingFound} />
}

export function WifiOfflineWarningNoNetworks() {
  return <Walk beats={[...nothingFound, { _tag: "ChoseOffline" }]} />
}

export function WifiCable() {
  return <Walk beats={started} start={{ ...onboardingModel, network: wiredNetwork }} />
}

export function WifiNoAdapter() {
  return <Walk beats={started} start={{ ...onboardingModel, network: noAdapterNetwork }} />
}

export function WifiHiddenNetwork() {
  return <Walk beats={[...scanned, { _tag: "ChoseHiddenNetwork" }, ...typed("Attic")]} />
}

export function WifiHiddenPassword() {
  return <Walk beats={[...scanned, { _tag: "ChoseHiddenNetwork" }, ...typed("Attic"), { _tag: "SubmittedText" }]} />
}

export function WifiNetworksWhileConnected() {
  return <Walk beats={[...connected, { _tag: "PickedAnotherNetwork" }]} />
}

export function Name() {
  return <Walk beats={naming} />
}

export function NameSaving() {
  return <Walk beats={named} />
}

export function NameRefused() {
  return (
    <Walk beats={[...named, {
      change: model => ({ ...model, settingsStatus: { _tag: "Problem", settingId: "device-name", message: "Names must be unique." } }),
    }]} />
  )
}

export function IdentityChoice() {
  return <Walk beats={choosing} />
}

export function IdentityChoiceOffline() {
  return <Walk beats={offlineChoosing} />
}

export function IdentityCreated() {
  return <Walk beats={created} />
}

export function IdentityBackup() {
  return <Walk beats={[...created, { _tag: "OpenedBackup" }]} />
}

// Filled in; RESTORE waits for the trust-loss question to be answered.
export function IdentityRestore() {
  return <Walk beats={restoring} />
}

export function IdentityRestoreConfirmed() {
  return <Walk beats={[...restoring, confirmed]} />
}

export function IdentityRestoreWorking() {
  return <Walk beats={[...restoring, confirmed, { _tag: "SubmittedIdentity" }]} />
}

export function IdentityRestoreRefused() {
  return (
    <Walk beats={[...restoring, confirmed, { _tag: "SubmittedIdentity" }, {
      change: model => model.identityManagement === undefined ? model : {
        ...model,
        identityManagement: { ...model.identityManagement, status: { _tag: "Problem", message: "That password does not open this backup." } },
      },
    }]} />
  )
}

export function IdentityRestored() {
  return <Walk beats={[...restoring, confirmed, { _tag: "SubmittedIdentity" }, "answer"]} />
}

export function IdentitySigner() {
  return <Walk beats={signing} />
}

export function IdentitySignerConfirmed() {
  return <Walk beats={[...signing, confirmed]} />
}

export function IdentityApproving() {
  return <Walk beats={[...signing, confirmed, { _tag: "SubmittedIdentity" }]} />
}

export function Controllers() {
  return <Walk beats={pads} />
}

export function ControllersReady() {
  return <Walk beats={padsReady} />
}

export function ControllersNone() {
  return <Walk beats={[...pads, { change: model => ({ ...model, onboarding: { controllers: [] } }) }]} />
}

export function GamesScanning() {
  return <Walk beats={[...games, { _tag: "ScannedGames" }]} />
}

export function Games() {
  return <Walk beats={games} />
}

export function GamesEmpty() {
  return <Walk beats={[...games, { change: model => ({ ...model, catalog: { _tag: "Empty" } }) }]} />
}

/* The Starter pack, offered above the library on a fresh device. With no
 * `starterPack` from Korri there is no offer at all: that is Games above. */
const install: Beat = { _tag: "ChoseStarterPackAction", actionId: fixtureStarterPackInstall.id }

export function GamesStarterPackOffered() {
  return <Walk beats={games} start={starterPackModel} />
}

export function GamesStarterPackOfferedWithLibrary() {
  return <Walk beats={games} start={withStarterPack(onboardingModel, starterPackOffered)} />
}

export function GamesStarterPackOfferedWhileScanning() {
  return <Walk beats={[...games, { _tag: "ScannedGames" }]} start={withStarterPack(onboardingModel, starterPackOffered)} />
}

export function GamesStarterPackInstalling() {
  return <Walk beats={[...games, install]} start={starterPackModel} />
}

export function GamesStarterPackInstalled() {
  return <Walk beats={[...games, install, "answer"]} start={starterPackModel} />
}

export function GamesStarterPackFailed() {
  return <Walk beats={[...games, install, { change: model => withStarterPack(model, starterPackFailed) }]} start={starterPackModel} />
}

export function GamesStarterPackFailedLongMessage() {
  return <Walk beats={[...games, install, { change: model => withStarterPack(model, starterPackFailedLong) }]} start={starterPackModel} />
}

export function Done() {
  return <Walk beats={done} />
}

export function DoneOffline() {
  return <Walk beats={offlineDone} />
}
