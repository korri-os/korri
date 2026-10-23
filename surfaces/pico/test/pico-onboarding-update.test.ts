/**
 * First-boot setup's rules, driven through `updateOnboarding` with plain
 * values: no DOM, no React, no host. Each test is a story: Korri's model,
 * messages in order, then what the player would see and what Pico asked.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel, SurfaceNetwork } from "@contracts/surface/korri-surface"
import {
  emptyScanNetwork,
  fixtureStarterPackInstall,
  fixtureStarterPackRetry,
  heardControllers,
  noAdapterNetwork,
  offlineNetwork,
  onboardingModel,
  onlineNetwork,
  personNameModel,
  starterPackFailed,
  starterPackInstalled,
  starterPackInstalling,
  starterPackOffered,
  unreachableNetwork,
  wiredNetwork,
  withStarterPack,
  wrongPasswordNetwork,
} from "../src/fixtures/onboarding"
import { picoOnboardingScreen, picoSetupCursor, picoSetupTrack, type PicoOnboardingScreen } from "../src/pico-onboarding-view"
import { PICO_SIGNAL_BAR_ABOVE, picoSignalBars } from "../src/pico-wifi-view"
import { initialOnboarding, type OnboardingMessage, type OnboardingNav } from "../src/state/onboarding"
import { updateOnboarding } from "../src/state/onboarding-update"
import type { PicoRequest } from "../src/state/requests"

function storyFrom(start: OnboardingNav, korri: SurfaceModel, ...messages: OnboardingMessage[]) {
  let nav = start
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = updateOnboarding(nav, message, korri)
    nav = step.model
    requests.push(...step.requests)
  }
  return { nav, requests, screen: picoOnboardingScreen(nav, korri) }
}
const story = (korri: SurfaceModel, ...messages: OnboardingMessage[]) => storyFrom(initialOnboarding(), korri, ...messages)

const withNetwork = (network: SurfaceNetwork, more: Partial<SurfaceModel> = {}): SurfaceModel =>
  ({ ...onboardingModel, network, ...more })
const typed = (text: string): OnboardingMessage[] => [...text].map(character => ({ _tag: "TypedText", character }))

const back = { _tag: "PressedBack" } as const
const start = { _tag: "Started" } as const
const next = { _tag: "Continued" } as const
const submit = { _tag: "SubmittedText" } as const
const published = { _tag: "KorriPublished" } as const

function wifiView(screen: PicoOnboardingScreen) {
  if (screen._tag !== "Wifi") throw new Error(`Expected the Wi-Fi card, saw ${screen._tag}`)
  return screen.view
}

/** Korri's model while the scan it was asked for runs. */
function scanningOf(korri: SurfaceModel): SurfaceModel {
  const network = korri.network
  if (network?.wifi._tag !== "Available") return korri
  return { ...korri, network: { ...network, wifi: { ...network.wifi, scanning: true } } }
}

/** Start, and let Korri answer the scan it was asked for. */
function scanned(korri: SurfaceModel = onboardingModel): OnboardingNav {
  const asked = story(korri, start).nav
  return storyFrom(storyFrom(asked, scanningOf(korri), published).nav, korri, published).nav
}

describe("Hello", () => {
  test("B does nothing; START opens Wi-Fi and asks Korri for a scan", () => {
    expect(story(onboardingModel, back).nav.step).toBe("hello")
    const { nav, requests, screen } = story(onboardingModel, start)
    expect(nav.step).toBe("wifi")
    expect(requests).toEqual([{ _tag: "ScanWifi" }])
    expect(wifiView(screen)._tag).toBe("Scanning")
  })

  test("names the Wi-Fi password only when the Wi-Fi step will ask for one", () => {
    expect(story(onboardingModel).screen).toEqual({ _tag: "Hello", askWifi: true })
    expect(story(withNetwork(wiredNetwork)).screen).toEqual({ _tag: "Hello", askWifi: false })
    expect(story(withNetwork(noAdapterNetwork)).screen).toEqual({ _tag: "Hello", askWifi: false })
  })

  test("a host that cannot see its network goes straight to the name, Wi-Fi marked passed", () => {
    const { network: _none, ...unmanaged } = onboardingModel
    const { nav } = story(unmanaged, start)
    expect(nav.step).toBe("name")
    expect(picoSetupTrack(nav, unmanaged)[1]?.state).toBe("skipped")
    expect(storyFrom(nav, unmanaged, back).nav.step).toBe("hello")
  })
})

describe("Wi-Fi is required", () => {
  test("the list offers no way past, and NEXT does nothing while offline", () => {
    const nav = scanned()
    const { nav: after, screen } = storyFrom(nav, onboardingModel, next, { _tag: "ChoseOffline" })
    expect(after.step).toBe("wifi")
    expect(wifiView(screen)._tag).toBe("Networks")
  })

  test("a cable is enough: no scan, the cable card, NEXT goes on and Wi-Fi counts as done", () => {
    const korri = withNetwork(wiredNetwork)
    const { nav, requests, screen } = story(korri, start)
    expect(requests).toEqual([])
    expect(wifiView(screen)).toEqual({ _tag: "Cable", wifi: true })
    const named = storyFrom(nav, korri, next).nav
    expect(named.step).toBe("name")
    expect(picoSetupTrack(named, korri)[1]?.state).toBe("done")
  })

  test("a device with no Wi-Fi goes on offline, and the track marks Wi-Fi as passed, not done", () => {
    const korri = withNetwork(noAdapterNetwork)
    const { nav, screen } = story(korri, start)
    expect(wifiView(screen)._tag).toBe("NoAdapter")
    const named = storyFrom(nav, korri, next).nav
    expect(named.step).toBe("name")
    expect(named.continuedOffline).toBe(true)
    expect(picoSetupTrack(named, korri).map(step => step.state)).toEqual(["done", "skipped", "now", "next", "next", "next", "next"])
  })
})

describe("joining a network", () => {
  test("a secured network asks for its password; CONNECT sends it and the password leaves the model", () => {
    const { nav: typing, screen } = storyFrom(scanned(), onboardingModel, { _tag: "PickedNetwork", networkId: "ap-home-5g" }, ...typed("pixel-Party8"))
    expect(screen._tag === "Entry" && screen.entry.secret && !screen.entry.shown).toBe(true)
    const shown = storyFrom(typing, onboardingModel, { _tag: "ToggledShown" }).screen
    expect(shown._tag === "Entry" && shown.entry.shown).toBe(true)
    const { nav, requests, screen: joining } = storyFrom(typing, onboardingModel, submit)
    expect(requests).toEqual([{ _tag: "ConnectWifi", networkId: "ap-home-5g", password: "pixel-Party8" }])
    expect(JSON.stringify(nav)).not.toContain("pixel-Party8")
    expect(wifiView(joining)).toEqual({ _tag: "Connecting", ssid: "Home-5G" })
  })

  test("a network Korri knows, or an open one, connects with no password", () => {
    expect(storyFrom(scanned(), onboardingModel, { _tag: "PickedNetwork", networkId: "ap-home-24" }).requests)
      .toEqual([{ _tag: "ConnectWifi", networkId: "ap-home-24" }])
    expect(storyFrom(scanned(), onboardingModel, { _tag: "PickedNetwork", networkId: "ap-cafe" }).requests)
      .toEqual([{ _tag: "ConnectWifi", networkId: "ap-cafe" }])
  })

  test("Back closes the password editor and drops what was typed", () => {
    const { nav } = storyFrom(scanned(), onboardingModel, { _tag: "PickedNetwork", networkId: "ap-home-5g" }, ...typed("secret1"), back)
    expect(nav.wifi.question._tag).toBe("None")
    expect(JSON.stringify(nav)).not.toContain("secret1")
  })

  test("a hidden network asks its name, then a password, which may be empty for an open one", () => {
    const { requests } = storyFrom(scanned(), onboardingModel, { _tag: "ChoseHiddenNetwork" }, ...typed("Attic"), submit, submit)
    expect(requests).toEqual([{ _tag: "ConnectHiddenWifi", ssid: "Attic" }])
  })

  test("Connecting holds until Korri answers, then Connected offers NEXT", () => {
    const joining = storyFrom(scanned(), onboardingModel, { _tag: "PickedNetwork", networkId: "ap-home-24" }).nav
    const busy = withNetwork(offlineNetwork.wifi._tag === "Available"
      ? { ...offlineNetwork, wifi: { ...offlineNetwork.wifi, attempt: { _tag: "Connecting", ssid: "Home-2.4" } } }
      : offlineNetwork)
    const seen = storyFrom(joining, busy, published)
    expect(wifiView(seen.screen)).toEqual({ _tag: "Connecting", ssid: "Home-2.4" })
    const online = withNetwork(onlineNetwork)
    const done = storyFrom(seen.nav, online, published)
    expect(wifiView(done.screen)).toEqual({ _tag: "Connected", ssid: "Home-5G" })
    expect(storyFrom(done.nav, online, next).nav.step).toBe("name")
  })
})

describe("continuing offline", () => {
  const refused = withNetwork(wrongPasswordNetwork)

  test("is offered after a rejected password, with Korri's words and Pico's hint about capitals", () => {
    const view = wifiView(story(refused, start).screen)
    expect(view).toEqual({
      _tag: "Failed",
      ssid: "NETGEAR-7B21",
      message: "NETGEAR-7B21 did not accept that password.",
      passwordRejected: true,
      fallback: { _tag: "Offline" },
    })
  })

  test("asks first, with the cursor on Back to Wi-Fi; B returns to the failure", () => {
    const { nav, screen } = story(refused, start, { _tag: "ChoseOffline" })
    expect(wifiView(screen)._tag).toBe("OfflineWarning")
    expect(picoSetupCursor(nav.step, screen).at).toBe("last")
    expect(wifiView(storyFrom(nav, refused, back).screen)._tag).toBe("Failed")
  })

  test("confirmed, setup goes on with Wi-Fi marked passed; Back from Name finds the failure again", () => {
    const { nav } = story(refused, start, { _tag: "ChoseOffline" }, { _tag: "ConfirmedOffline" })
    expect(nav.step).toBe("name")
    expect(picoSetupTrack(nav, refused)[1]?.state).toBe("skipped")
    expect(wifiView(storyFrom(nav, refused, back).screen)._tag).toBe("Failed")
  })

  test("is offered when nothing was found", () => {
    const empty = withNetwork(emptyScanNetwork)
    const { screen } = storyFrom(scanned(empty), empty, { _tag: "ChoseOffline" })
    expect(wifiView(screen)._tag).toBe("OfflineWarning")
  })

  test("is not offered after a failure while another connection works", () => {
    const failedOnline = withNetwork({ ...unreachableNetwork, connection: { _tag: "Wifi", ssid: "Home-5G" } })
    const { screen } = story(failedOnline, start, { _tag: "ChoseOffline" })
    const view = wifiView(screen)
    expect(view._tag === "Failed" && view.fallback).toEqual({ _tag: "Stay", label: "Stay on Home-5G" })
  })

  test("Try again opens a new, empty password; Pick another shows the list without the failure", () => {
    const retry = storyFrom(scanned(refused), refused, { _tag: "TriedAgain" }).screen
    expect(retry._tag === "Entry" && retry.entry.draft.text).toBe("")
    const list = storyFrom(scanned(refused), refused, { _tag: "PickedAnotherNetwork" }).screen
    expect(wifiView(list)._tag).toBe("Networks")
  })
})

describe("the name", () => {
  const named: SurfaceModel = {
    ...onboardingModel,
    network: onlineNetwork,
    settings: onboardingModel.settings.map(group => ({
      ...group,
      items: group.items.map(item => (item.id === "device-name" ? { ...item, value: "usu" } : item)),
    })),
  }

  test("starts from Korri's name when it has one, else empty with the placeholder", () => {
    const prefilled = story(named, start, next).screen
    expect(prefilled._tag === "Entry" && prefilled.entry.draft.text).toBe("usu")
    const fresh = story(withNetwork(onlineNetwork), start, next).screen
    expect(fresh._tag === "Entry" && [fresh.entry.draft.text, fresh.entry.placeholder]).toEqual(["", "This device"])
  })

  test("NEXT saves it through the device-name setting and waits for Korri's save", () => {
    const korri = withNetwork(onlineNetwork)
    const { nav, requests } = story(korri, start, next, ...typed("Pocket"), submit)
    expect(requests.at(-1)).toEqual({ _tag: "ChangeSetting", settingId: "device-name", value: "Pocket" })
    expect(nav.step).toBe("name")
    const saving = storyFrom(nav, { ...korri, settingsStatus: { _tag: "Saving", settingId: "device-name" } }, published).nav
    expect(saving.step).toBe("name")
    expect(storyFrom(saving, korri, published).nav.step).toBe("you")
  })

  test("an unchanged name goes straight on", () => {
    const { nav, requests } = story(named, start, next, submit)
    expect(nav.step).toBe("you")
    expect(requests.some(request => request._tag === "ChangeSetting")).toBe(false)
  })
})

describe("who you are", () => {
  const online = withNetwork(onlineNetwork)
  const atYou = (korri: SurfaceModel) => story(korri, start, next, ...typed("Pocket"), submit, published).nav
  function onYou(korri: SurfaceModel): OnboardingNav {
    const nav = atYou(korri)
    return nav.step === "you" ? nav : { ...nav, step: "you" }
  }

  test("a new identity keeps the one Korri made: no host call, and the backup is offered", () => {
    const { nav, requests, screen } = storyFrom(onYou(online), online, { _tag: "ChoseIdentity", how: "created" })
    expect(requests).toEqual([])
    expect(screen).toEqual({ _tag: "Identity", view: { _tag: "Ready", origin: "created", canBackUp: true } })
    expect(storyFrom(nav, online, next).nav.step).toBe("pads")
  })

  test("offline, restoring and a signer are off and say why", () => {
    const offline = withNetwork(noAdapterNetwork)
    const { screen, requests } = storyFrom(onYou(offline), offline, { _tag: "ChoseIdentity", how: "restored" })
    expect(requests).toEqual([])
    if (screen._tag !== "Identity" || screen.view._tag !== "Choose") throw new Error("Expected the choice")
    expect(screen.view.restore).toEqual({ enabled: false, note: "Needs a connection." })
    expect(screen.view.signer).toEqual({ enabled: false, note: "Needs a connection." })
  })

  const restoreFilled: OnboardingMessage[] = [
    { _tag: "ChoseIdentity", how: "restored" },
    { _tag: "EditedIdentityField", field: "backup" }, ...typed("ncryptsec1abc"), submit,
    { _tag: "EditedIdentityField", field: "password" }, ...typed("hunter22"), submit,
  ]

  test("a switch waits for the player to confirm the trust loss; Pico never answers it", () => {
    const { requests, screen } = storyFrom(onYou(online), online, ...restoreFilled, { _tag: "SubmittedIdentity" })
    expect(requests.some(request => request._tag === "SwitchIdentityFromBackup")).toBe(false)
    if (screen._tag !== "Identity" || screen.view._tag !== "Restore") throw new Error("Expected the restore form")
    expect([screen.view.confirmed, screen.view.ready]).toEqual([false, false])
    const signer = storyFrom(onYou(online), online, { _tag: "ChoseIdentity", how: "signer" },
      { _tag: "EditedIdentityField", field: "uri" }, ...typed("bunker://k"), submit, { _tag: "SubmittedIdentity" })
    expect(signer.requests.some(request => request._tag === "SwitchIdentityToNip46")).toBe(false)
  })

  test("turned on and off again, the statement still holds the switch", () => {
    const { requests } = storyFrom(onYou(online), online, ...restoreFilled,
      { _tag: "ToggledTrustLoss" }, { _tag: "ToggledTrustLoss" }, { _tag: "SubmittedIdentity" })
    expect(requests.some(request => request._tag === "SwitchIdentityFromBackup")).toBe(false)
  })

  test("restore sends the backup, the password and the player's yes, forgets the password, and is ready when Korri switched", () => {
    const { nav, requests } = storyFrom(onYou(online), online, ...restoreFilled,
      { _tag: "ToggledTrustLoss" }, { _tag: "SubmittedIdentity" })
    expect(requests.at(-1)).toEqual({
      _tag: "SwitchIdentityFromBackup", encryptedSecret: "ncryptsec1abc", password: "hunter22", disposition: "transfer", trustLossConfirmed: true,
    })
    expect(JSON.stringify(nav)).not.toContain("hunter22")
    const switched = { ...online, identityManagement: { localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Switched", ownerPublicKey: "k" } } } as const
    expect(storyFrom(nav, switched, published).screen).toEqual({ _tag: "Identity", view: { _tag: "Ready", origin: "restored", canBackUp: false } })
  })

  test("a signer is asked through NIP-46", () => {
    const { requests } = storyFrom(onYou(online), online,
      { _tag: "ChoseIdentity", how: "signer" },
      { _tag: "EditedIdentityField", field: "uri" }, ...typed("bunker://k"), submit,
      { _tag: "ToggledTrustLoss" }, { _tag: "SubmittedIdentity" })
    expect(requests.at(-1)).toEqual({ _tag: "SwitchIdentityToNip46", bunkerUri: "bunker://k", disposition: "transfer", trustLossConfirmed: true })
  })
})

describe("pads, games and play", () => {
  const korri = withNetwork(onlineNetwork)
  const at = (step: OnboardingNav["step"]): OnboardingNav => ({ ...initialOnboarding(), step, identity: { _tag: "Ready", origin: "created" } })

  test("NEXT waits for a controller Korri heard; seats change through Korri's setting", () => {
    expect(storyFrom(at("pads"), korri, next).nav.step).toBe("pads")
    const heard = { ...korri, onboarding: { controllers: heardControllers } }
    expect(storyFrom(at("pads"), heard, next).nav.step).toBe("games")
    expect(storyFrom(at("pads"), korri, { _tag: "ChangedSetting", settingId: "host.preferences.playerCount", value: "5" }).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "host.preferences.playerCount", value: "5" }])
  })

  test("a host that lists no controllers does not hold setup", () => {
    expect(storyFrom(at("pads"), { ...korri, onboarding: { controllers: [] } }, next).nav.step).toBe("games")
  })

  test("Scan again runs Korri's rescan action", () => {
    expect(storyFrom(at("games"), korri, { _tag: "ScannedGames" }).requests).toEqual([{ _tag: "RunAction", actionId: "game-folder-rescan" }])
  })

  test("the summary states Korri's facts, and the network as Offline when it is", () => {
    const offline = withNetwork(noAdapterNetwork)
    const screen = storyFrom(at("play"), offline).screen
    if (screen._tag !== "Play") throw new Error("Expected Play")
    expect(screen.facts[0]).toEqual({ label: "Network", value: "Offline" })
    expect(screen.facts.find(fact => fact.label === "Player seats")?.value).toBe("4")
  })

  test("GO TO GAMES asks Korri once to finish; Back then waits", () => {
    const { nav, requests } = storyFrom(at("play"), korri, { _tag: "Finished" }, { _tag: "Finished" }, back)
    expect(requests).toEqual([{ _tag: "FinishOnboarding" }])
    expect(nav.step).toBe("play")
  })
})

describe("the Starter pack", () => {
  const korri = withNetwork(onlineNetwork)
  const games: OnboardingNav = { ...initialOnboarding(), step: "games", identity: { _tag: "Ready", origin: "created" } }
  const install = { _tag: "ChoseStarterPackAction", actionId: fixtureStarterPackInstall.id } as const
  const retry = { _tag: "ChoseStarterPackAction", actionId: fixtureStarterPackRetry.id } as const
  function gamesScreen(screen: PicoOnboardingScreen) {
    if (screen._tag !== "Games") throw new Error(`Expected Games, saw ${screen._tag}`)
    return screen
  }

  test("Install runs the action Korri published with the offer", () => {
    const offered = withStarterPack(korri, starterPackOffered)
    const { nav, requests, screen } = storyFrom(games, offered, install)
    expect(requests).toEqual([{ _tag: "RunAction", actionId: "starter-pack-install" }])
    expect(nav.step).toBe("games")
    expect(gamesScreen(screen).starterPack).toEqual({
      _tag: "Offered",
      actions: [{ id: "starter-pack-install", label: "Install Starter pack", enabled: true }],
    })
  })

  test("after a failure, Korri's words show and the retry runs Korri's retry action", () => {
    const failed = withStarterPack(korri, starterPackFailed)
    const { requests, screen } = storyFrom(games, failed, retry)
    expect(requests).toEqual([{ _tag: "RunAction", actionId: "starter-pack-retry" }])
    expect(gamesScreen(screen).starterPack).toEqual({
      _tag: "Failed",
      message: "The Starter pack could not be installed. Try again.",
      actions: [{ id: "starter-pack-retry", label: "Try Starter pack again", enabled: true }],
    })
  })

  test("asks nothing for an action Korri does not offer now, or one it dimmed", () => {
    expect(storyFrom(games, withStarterPack(korri, starterPackInstalling), install).requests).toEqual([])
    expect(storyFrom(games, withStarterPack(korri, starterPackInstalled), install).requests).toEqual([])
    expect(storyFrom(games, withStarterPack(korri, starterPackOffered), retry).requests).toEqual([])
    const dimmed = withStarterPack(korri, { _tag: "Offered", actions: [{ ...fixtureStarterPackInstall, enabled: false }] })
    expect(storyFrom(games, dimmed, install).requests).toEqual([])
    // Only on the games step.
    expect(storyFrom({ ...games, step: "pads" }, withStarterPack(korri, starterPackOffered), install).requests).toEqual([])
  })

  test("NEXT goes on while the pack installs: it finishes by itself", () => {
    const installing = withStarterPack(korri, starterPackInstalling)
    expect(gamesScreen(storyFrom(games, installing).screen).starterPack).toEqual({ _tag: "Installing" })
    expect(storyFrom(games, installing, next).nav.step).toBe("play")
  })

  test("with no starterPack from Korri there is no offer", () => {
    expect(gamesScreen(storyFrom(games, korri).screen).starterPack).toEqual({ _tag: "None" })
    expect(storyFrom(games, korri, install).requests).toEqual([])
  })

  /** Korri reading the library: its rescan under way. */
  const rescanning = (model: SurfaceModel): SurfaceModel =>
    ({ ...model, settingsStatus: { _tag: "Saving", settingId: "game-folder-rescan" } })

  test("Scan again during the install reads the library, and NEXT still goes on", () => {
    const installing = withStarterPack(korri, starterPackInstalling)
    expect(storyFrom(games, installing, { _tag: "ScannedGames" }).requests)
      .toEqual([{ _tag: "RunAction", actionId: "game-folder-rescan" }])
    const both = rescanning(installing)
    expect(gamesScreen(storyFrom(games, both).screen).view).toEqual({ _tag: "Scanning", canContinue: true })
    expect(storyFrom(games, both, next).nav.step).toBe("play")
    // Korri's own read of the catalog, as after the pack is on, counts the same.
    expect(storyFrom(games, { ...installing, catalog: { _tag: "Loading" } }, next).nav.step).toBe("play")
  })

  test("with no install under way, a library read still holds NEXT", () => {
    for (const pack of [undefined, starterPackOffered, starterPackInstalled, starterPackFailed]) {
      const scanning = rescanning(withStarterPack(korri, pack))
      expect(gamesScreen(storyFrom(games, scanning).screen).view).toEqual({ _tag: "Scanning", canContinue: false })
      expect(storyFrom(games, scanning, next).nav.step).toBe("games")
    }
  })

  test("the cursor starts again when the offer changes, so it never rests on a row that went", () => {
    const key = (model: SurfaceModel) => picoSetupCursor("games", storyFrom(games, model).screen).key
    const keys = [
      korri,
      withStarterPack(korri, starterPackOffered),
      withStarterPack(korri, starterPackInstalling),
      withStarterPack(korri, starterPackInstalled),
      withStarterPack(korri, starterPackFailed),
    ].map(key)
    expect(new Set(keys).size).toBe(keys.length)
  })

})

describe("the person name", () => {
  // Korri lets this device change the person name: a text setting, at most 64.
  const korri: SurfaceModel = { ...personNameModel, network: onlineNetwork }
  const ready = (): OnboardingNav => ({ ...initialOnboarding(), step: "you", identity: { _tag: "Ready", origin: "created" } })
  const withPersonName = (value: string | undefined, interaction: boolean): SurfaceModel => ({
    ...korri,
    settings: korri.settings.map(group => ({
      ...group,
      items: group.items.map(item => item.id !== "person-name" ? item : {
        id: item.id,
        label: item.label,
        ...(value === undefined ? {} : { value }),
        ...(interaction && item.interaction !== undefined ? { interaction: item.interaction } : {}),
      }),
    })),
  })

  test("comes after the identity, as part of YOU on the track, starting empty with its placeholder", () => {
    const { nav, screen } = storyFrom(ready(), korri, next)
    expect(nav.step).toBe("person")
    expect(screen._tag === "Entry" && [screen.entry.place, screen.entry.draft.text, screen.entry.placeholder, screen.entry.maxLength])
      .toEqual(["YOUR NAME", "", "Your name", 64])
    expect(picoSetupTrack(nav, korri).find(step => step.state === "now")?.label).toBe("YOU")
  })

  test("is skipped where Korri does not publish it, or publishes it read-only", () => {
    expect(storyFrom(ready(), withNetwork(onlineNetwork), next).nav.step).toBe("pads")
    const readOnly = withPersonName("Simon", false)
    expect(storyFrom(ready(), readOnly, next).nav.step).toBe("pads")
    expect(storyFrom({ ...ready(), step: "pads" }, readOnly, back).nav.step).toBe("you")
  })

  test("starts from the name Korri holds", () => {
    const named = withPersonName("Simon", true)
    const { screen } = storyFrom(ready(), named, next)
    expect(screen._tag === "Entry" && screen.entry.draft.text).toBe("Simon")
  })

  test("NEXT waits for a name, then saves it through person-name and goes on once Korri saved", () => {
    const empty = storyFrom(ready(), korri, next, submit)
    expect(empty.nav.step).toBe("person")
    expect(empty.requests).toEqual([])
    const { nav, requests } = storyFrom(ready(), korri, next, ...typed("Simon"), submit)
    expect(requests.at(-1)).toEqual({ _tag: "ChangeSetting", settingId: "person-name", value: "Simon" })
    const saving = storyFrom(nav, { ...korri, settingsStatus: { _tag: "Saving", settingId: "person-name" } }, published).nav
    expect(saving.step).toBe("person")
    expect(storyFrom(saving, korri, published).nav.step).toBe("pads")
  })

  test("a refusal keeps the step and the typing, with Korri's words", () => {
    const { nav } = storyFrom(ready(), korri, next, ...typed("Simon"), submit)
    const refused = { ...korri, settingsStatus: { _tag: "Problem", settingId: "person-name", message: "Your signer did not answer." } } as const
    const after = storyFrom(nav, refused, published)
    expect(after.nav.step).toBe("person")
    expect(after.screen._tag === "Entry" && [after.screen.entry.draft.text, after.screen.entry.problem]).toEqual(["Simon", "Your signer did not answer."])
  })

  test("the keys stop at Korri's limit", () => {
    const { screen } = storyFrom(ready(), korri, next, ...typed("x".repeat(70)))
    expect(screen._tag === "Entry" && screen.entry.draft.text.length).toBe(64)
  })

  test("an unchanged name goes straight on; Back walks pads, the name, then YOU", () => {
    const named = withPersonName("Simon", true)
    const { nav, requests } = storyFrom(ready(), named, next, submit)
    expect(nav.step).toBe("pads")
    expect(requests).toEqual([])
    const onName = storyFrom(nav, named, back)
    expect(onName.nav.step).toBe("person")
    expect(onName.screen._tag === "Entry" && onName.screen.entry.draft.text).toBe("Simon")
    expect(storyFrom(onName.nav, named, back).nav.step).toBe("you")
  })
})

describe("signal bars", () => {
  test("follow nmcli's cut points", () => {
    expect(PICO_SIGNAL_BAR_ABOVE).toEqual([5, 30, 55, 80])
    expect([0, 5, 6, 30, 31, 55, 56, 80, 81, 100].map(picoSignalBars)).toEqual([0, 0, 1, 1, 2, 2, 3, 3, 4, 4])
  })
})
