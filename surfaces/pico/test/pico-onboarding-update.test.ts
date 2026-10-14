/**
 * First-boot setup's rules, driven through `updateOnboarding` with plain
 * values: no DOM, no React, no host. Each test is a story: Korri's model,
 * messages in order, then what the player would see and what Pico asked.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel, SurfaceNetwork } from "@contracts/surface/korri-surface"
import {
  emptyScanNetwork,
  heardControllers,
  noAdapterNetwork,
  offlineNetwork,
  onboardingModel,
  onlineNetwork,
  unreachableNetwork,
  wiredNetwork,
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

  test("restore sends the backup and password, forgets the password, and is ready when Korri switched", () => {
    const { nav, requests } = storyFrom(onYou(online), online,
      { _tag: "ChoseIdentity", how: "restored" },
      { _tag: "EditedIdentityField", field: "backup" }, ...typed("ncryptsec1abc"), submit,
      { _tag: "EditedIdentityField", field: "password" }, ...typed("hunter22"), submit,
      { _tag: "SubmittedIdentity" })
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
      { _tag: "SubmittedIdentity" })
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

describe("signal bars", () => {
  test("follow nmcli's cut points", () => {
    expect(PICO_SIGNAL_BAR_ABOVE).toEqual([5, 30, 55, 80])
    expect([0, 5, 6, 30, 31, 55, 56, 80, 81, 100].map(picoSignalBars)).toEqual([0, 0, 1, 1, 2, 2, 3, 3, 4, 4])
  })
})
