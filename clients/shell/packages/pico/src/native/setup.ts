/**
 * First-boot setup (PicoOnboarding): the step track, then one card per step
 * in setup's words, with its rows and answers. The rules are Pico's own
 * (src/state/onboarding-update.ts); this file draws and places the cursor.
 */
import {
  picoOnboardingScreen,
  picoSetupCursor,
  picoSetupPlace,
  picoSetupTrack,
  starterPackChoices,
  type PicoOnboardingScreen,
} from "../pico-onboarding-view"
import type { PicoNetworkView } from "../pico-wifi-view"
import { PICO_IDENTITY_BACKUP_ACTION } from "../pico-settings-view"
import { IDENTITY_FORM } from "../state/navigation"
import type { OnboardingMessage } from "../state/onboarding"
import type { Target } from "./focus"
import { frame } from "./frame"
import { A, B, button, type ButtonView, cartView, type CartView, NO_CARD, type HintView } from "./kit"
import { editorParts, identityScreen, type RowView, rowView, stacked, typingScreen } from "./panel"
import { type Act, type Ctx, local, type Screen, setup } from "./types"

interface Answer {
  readonly id: string
  readonly label: string
  readonly detail?: string
  readonly disabled?: boolean
  readonly message?: OnboardingMessage
  readonly act?: Act
}

export interface SetupView {
  /** "hello", "card" or "done". */
  readonly layout: string
  readonly tone: string
  readonly kicker: string
  readonly title: string
  readonly message: string
  readonly busy: string
  readonly rows: readonly RowView[]
  readonly carts: readonly CartView[]
  readonly more: string
  readonly answers: readonly ButtonView[]
  /** Answers side by side, under the rows (Hidden network, Scan again). */
  readonly inline: boolean
  readonly go: ButtonView
  readonly hasGo: boolean
  /** The Starter pack's own card. */
  readonly pack: { readonly shown: boolean; readonly tone: string; readonly title: string; readonly message: string; readonly answers: readonly ButtonView[] }
}

const EMPTY_PACK = { shown: false, tone: "ask", title: "", message: "", answers: [] as ButtonView[] }

function networkRow(network: PicoNetworkView, focused: boolean): RowView {
  return rowView({
    id: network.id,
    label: network.ssid,
    control: { kind: "network", actionId: network.id, network },
    state: "idle",
  }, focused)
}

const SELECT_BACK: readonly HintView[] = [A("SELECT"), B("BACK")]

export function setupScreen(ctx: Ctx): Screen {
  const { s, model } = ctx
  const focused = s.focus
  const screen = picoOnboardingScreen(s.setup, model)
  const cursor = picoSetupCursor(s.setup.step, screen)
  const track = picoSetupTrack(s.setup, model).map(step => ({ state: step.state }))
  const place = picoSetupPlace(s.setup.step, screen)
  const base = (hints: readonly HintView[]) => frame({ place: "SETUP", label: place, hints, track })

  // The backup dialog over the identity step, and the keyboard over a field.
  if (s.typing !== undefined) {
    const shown = typingScreen(ctx, s.typing, "SETUP")
    return { ...shown, view: { ...shown.view, layer: "setup", frame: { ...(shown.view["frame"] as object), track } } }
  }
  if (screen._tag === "Identity" && screen.backup !== undefined) {
    const backup = screen.backup
    const shown = identityScreen(
      { ...ctx, model: { ...model, ...(backup.identity === undefined ? {} : { identityManagement: backup.identity }) } },
      { _tag: "Identity", actionId: PICO_IDENTITY_BACKUP_ACTION, form: { ...IDENTITY_FORM, password: backup.password }, qr: undefined },
    )
    // The dialog's answers are setup's: the password field, CREATE, CLOSE.
    const targets = shown.targets.map(target =>
      target.id === "identity:password"
        ? { ...target, act: [local(state => ({ ...state, typing: { field: "backup:password" as const, label: "Password", draft: { text: backup.password, capitals: false, symbols: false }, secret: true } }))] }
        : target.id === "submit" ? { ...target, act: [setup({ _tag: "SubmittedBackup" })] }
        : target.id === "close" || target.id === "done" ? { ...target, act: [setup({ _tag: "ClosedBackup" })] }
        : target)
    return { ...shown, targets, view: { ...shown.view, layer: "setup", frame: base(SELECT_BACK) } }
  }
  if (screen._tag === "Entry") {
    const entry = screen.entry
    const parts = editorParts({
      group: place,
      note: entry.note ?? "",
      prompt: entry.prompt,
      draft: entry.draft,
      secret: entry.secret,
      shown: entry.shown,
      placeholder: entry.placeholder,
      ...(entry.maxLength === undefined ? {} : { maxLength: entry.maxLength }),
      actions: [
        { label: entry.busy ? "SAVING" : entry.submitLabel, disabled: !entry.ready || entry.busy, act: [setup({ _tag: "SubmittedText" })] },
        ...(entry.secret ? [{ label: entry.shown ? "HIDE" : "SHOW", act: [setup({ _tag: "ToggledShown" })] }] : []),
        ...(entry.problem === undefined ? [] : [{ label: "OK", act: [setup({ _tag: "DismissedSettingsProblem" })] }]),
      ],
      ...(entry.problem === undefined ? {} : { problem: entry.problem }),
    }, focused, press =>
      press._tag === "Type" ? [setup({ _tag: "TypedText", character: press.character })]
        : press._tag === "Backspace" ? [setup({ _tag: "PressedTextBackspace" })]
        : press._tag === "Clear" ? [setup({ _tag: "ClearedText" })]
        : press._tag === "Capitals" ? [setup({ _tag: "ToggledCapitals" })]
        : [setup({ _tag: "ToggledSymbols" })])
    return {
      key: cursor.key,
      targets: parts.targets,
      fallback: "key:0:0",
      view: { layer: "setup", page: "editor", card: NO_CARD, editor: parts.editor, frame: base(SELECT_BACK) },
    }
  }

  const shown = describe(screen)
  const targets: Target<Act>[] = []
  const rows = shown.networks.map((network, i) => {
    const id = `network:${network.id}`
    targets.push({ id, box: { x: 0, y: 10 + i * 10, w: 100, h: 8 }, act: [setup({ _tag: "PickedNetwork", networkId: network.id })] })
    return networkRow(network, focused === id)
  })
  const factRows = shown.facts.map(fact => rowView({ id: fact.label, label: fact.label, value: fact.value, control: { kind: "fact" }, state: "idle" }, false))
  const answers = shown.answers.map((answer, i) => {
    if (answer.disabled !== true) {
      targets.push({
        id: answer.id,
        box: shown.inline ? { x: i * 50, y: 500, w: 48, h: 8 } : { x: 0, y: 500 + i * 10, w: 100, h: 8 },
        act: answer.act ?? (answer.message === undefined ? [] : [setup(answer.message)]),
      })
    }
    return button(answer.label, { detail: answer.detail ?? "", disabled: answer.disabled === true, focused: focused === answer.id })
  })
  const pack = shown.pack === undefined ? EMPTY_PACK : {
    ...shown.pack,
    answers: shown.pack.answers.map((answer, i) => {
      if (answer.disabled !== true) targets.push({ id: answer.id, box: { x: 0, y: 300 + i * 10, w: 100, h: 8 }, act: answer.message === undefined ? [] : [setup(answer.message)] })
      return button(answer.label, { disabled: answer.disabled === true, focused: focused === answer.id })
    }),
  }
  if (shown.go !== undefined && shown.go.disabled !== true) {
    targets.push({ id: shown.go.id, box: { x: 0, y: 700, w: 30, h: 10 }, act: shown.go.message === undefined ? [] : [setup(shown.go.message)] })
  }
  const enabled = targets.map(target => target.id)
  const fallback = cursor.at === "last" ? enabled[enabled.length - 1] : enabled[0]
  return {
    key: cursor.key,
    targets,
    ...(fallback === undefined ? {} : { fallback }),
    view: {
      layer: "setup",
      page: "setup",
      card: NO_CARD,
      frame: base(shown.hints),
      setup: {
        step: screen._tag === "Hello" ? "hello" : screen._tag === "Wifi" ? "wifi" : screen._tag === "Identity" ? "you" : screen._tag === "Pads" ? "pads" : screen._tag === "Games" ? "games" : "play",
        layout: shown.layout,
        tone: shown.tone,
        kicker: shown.kicker,
        title: shown.title,
        message: shown.message,
        busy: shown.busy,
        rows: stacked([...rows, ...factRows]),
        carts: shown.carts,
        more: shown.more,
        answers,
        inline: shown.inline,
        go: button(shown.go?.label ?? "", { focused: shown.go !== undefined && focused === shown.go.id, disabled: shown.go?.disabled === true }),
        hasGo: shown.go !== undefined,
        pack,
      },
    },
  }
}

interface Described {
  readonly layout: "hello" | "card" | "done"
  readonly tone: string
  readonly kicker: string
  readonly title: string
  readonly message: string
  readonly busy: string
  readonly networks: readonly PicoNetworkView[]
  readonly facts: readonly { readonly label: string; readonly value: string }[]
  readonly carts: readonly CartView[]
  readonly more: string
  readonly answers: readonly Answer[]
  readonly inline: boolean
  readonly go?: { readonly id: string; readonly label: string; readonly disabled?: boolean; readonly message?: OnboardingMessage }
  readonly pack?: { readonly shown: boolean; readonly tone: string; readonly title: string; readonly message: string; readonly answers: readonly Answer[] }
  readonly hints: readonly HintView[]
}

const card = (options: Partial<Described>): Described => ({
  layout: "card",
  tone: "ask",
  kicker: "",
  title: "",
  message: "",
  busy: "",
  networks: [],
  facts: [],
  carts: [],
  more: "",
  answers: [],
  inline: false,
  hints: SELECT_BACK,
  ...options,
})

/** Each step in setup's own words (PicoSetupWelcome, PicoWifiSetup,
 * PicoIdentitySetup, PicoControllerCheck, PicoGamesFound, PicoSetupDone). */
function describe(screen: Exclude<PicoOnboardingScreen, { _tag: "Entry" }>): Described {
  switch (screen._tag) {
    case "Hello":
      return card({
        layout: "hello",
        title: "Hello.",
        message: screen.askWifi
          ? "Let's get this console ready to play. Have your Wi-Fi password handy."
          : "Let's get this console ready to play. It takes about a minute.",
        go: { id: "start", label: "START", message: { _tag: "Started" } },
        hints: [A("START")],
      })
    case "Wifi": {
      const view = screen.view
      const hints = view._tag === "Scanning" || view._tag === "Connecting" ? [B("BACK")] : SELECT_BACK
      switch (view._tag) {
        case "Scanning":
          return card({ tone: "tell", title: "Looking for networks", message: "This takes a few seconds.", busy: "Looking for networks", hints })
        case "Networks":
          return card({
            title: "Pick a network",
            message: view.connectedSsid === undefined ? "Setup needs a connection to go on." : `On ${view.connectedSsid} now.`,
            networks: view.networks,
            inline: true,
            answers: [
              { id: "hidden", label: "Hidden network", message: { _tag: "ChoseHiddenNetwork" } },
              { id: "scan", label: "Scan again", message: { _tag: "ScannedNetworks" } },
            ],
            hints,
          })
        case "Connecting":
          return card({ tone: "tell", title: "Connecting", message: `Joining ${view.ssid}.`, busy: "Connecting", hints })
        case "Connected":
          return card({
            tone: "tell",
            title: "Connected",
            message: `This device is online through ${view.ssid}.`,
            answers: [
              { id: "next", label: "NEXT", message: { _tag: "Continued" } },
              { id: "change", label: "Change network", message: { _tag: "PickedAnotherNetwork" } },
            ],
            hints,
          })
        case "Failed":
          return card({
            tone: "warn",
            title: view.passwordRejected ? "Wrong password" : "Could not connect",
            message: view.passwordRejected ? `${view.message} Capitals count: turn on SHOW to check it.` : view.message,
            answers: [
              { id: "again", label: "Try again", message: { _tag: "TriedAgain" } },
              { id: "another", label: "Pick another network", message: { _tag: "PickedAnotherNetwork" } },
              view.fallback._tag === "Offline"
                ? { id: "offline", label: "Continue offline", message: { _tag: "ChoseOffline" } }
                : { id: "stay", label: view.fallback.label, message: { _tag: "StayedConnected" } },
            ],
            hints,
          })
        case "NoNetworks":
          return card({
            tone: "warn",
            title: "No networks found",
            message: "Move closer to the router and scan again, or join a hidden network by name. A network cable also works.",
            answers: [
              { id: "scan", label: "Scan again", message: { _tag: "ScannedNetworks" } },
              { id: "hidden", label: "Hidden network", detail: "▶", message: { _tag: "ChoseHiddenNetwork" } },
              { id: "offline", label: "Continue offline", message: { _tag: "ChoseOffline" } },
            ],
            hints,
          })
        case "Cable":
          return card({
            tone: "tell",
            title: "Online by cable",
            message: view.wifi
              ? "This device is online through a network cable. Set up Wi-Fi too if it will leave the cable."
              : "This device is online through a network cable.",
            answers: [
              { id: "next", label: "NEXT", message: { _tag: "Continued" } },
              ...(view.wifi ? [{ id: "wifi", label: "Set up Wi-Fi too", message: { _tag: "ChoseWifiToo" } as const }] : []),
            ],
            hints,
          })
        case "NoAdapter":
          return card({
            tone: "tell",
            title: "No Wi-Fi here",
            message: "This device has no Wi-Fi. Plug in a network cable to go online, or carry on offline: games on this device still play.",
            answers: [{ id: "offline", label: "Continue offline", message: { _tag: "Continued" } }],
            hints,
          })
        case "OfflineWarning":
          return card({
            title: "Continue offline?",
            message: "With no connection, this device cannot restore an identity from a backup or connect a signer. Games on this device still play.",
            answers: [
              { id: "offline", label: "Continue offline", message: { _tag: "ConfirmedOffline" } },
              { id: "back", label: "Back to Wi-Fi", message: { _tag: "ReturnedToWifi" } },
            ],
            hints,
          })
      }
      break
    }
    case "Identity": {
      const view = screen.view
      const none = view._tag === "Working" || view._tag === "Approving"
      const hints = none ? [] : SELECT_BACK
      const field = (id: "backup" | "password" | "uri", label: string, value: string): Answer => ({
        id: `field:${id}`,
        label,
        detail: value === "" ? "EMPTY" : value,
        message: { _tag: "EditedIdentityField", field: id },
      })
      const statement: Answer = {
        id: "trust",
        label: "I understand that peer pairings and stream-client trust will be cleared.",
        message: { _tag: "ToggledTrustLoss" },
      }
      switch (view._tag) {
        case "Choose":
          return card({
            title: "Who are you?",
            message: view.offline
              ? "Your identity is the key that ties your Korri devices together. With no connection, only a new one can be made."
              : "Your identity is the key that ties your Korri devices together.",
            answers: [
              { id: "new", label: "New identity", message: { _tag: "ChoseIdentity", how: "created" } },
              { id: "restore", label: "Restore from backup", detail: view.restore.note, disabled: !view.restore.enabled, message: { _tag: "ChoseIdentity", how: "restored" } },
              { id: "signer", label: "Connect a signer", detail: view.signer.note, disabled: !view.signer.enabled, message: { _tag: "ChoseIdentity", how: "signer" } },
            ],
            hints,
          })
        case "Working":
          return card({ tone: "tell", title: "One moment", message: view.label, busy: view.label, hints })
        case "Ready":
          return card({
            tone: "tell",
            title: view.origin === "created" ? "Identity created" : view.origin === "restored" ? "Identity restored" : "Signer connected",
            message: view.origin === "created"
              ? `Korri made a key for you on this device. ${view.canBackUp ? "Korri cannot recover a lost key. Back it up now, or later from Settings." : "Korri cannot recover a lost key. Back it up later from Settings."}`
              : view.origin === "restored" ? "This device uses the identity from your backup."
              : "Your signer keeps the key. Korri asks it each time it needs to sign.",
            answers: [
              { id: "next", label: "NEXT", message: { _tag: "Continued" } },
              ...(view.canBackUp ? [{ id: "backup", label: "Back up now", detail: "▶", message: { _tag: "OpenedBackup" } as const }] : []),
            ],
            hints,
          })
        case "Restore":
          return card({
            title: "Restore from backup",
            message: [view.problem, "Type the encrypted backup and the password it was made with."].filter(part => part !== undefined).join(" "),
            answers: [
              field("backup", "Backup", view.backup.length > 16 ? `${view.backup.slice(0, 16)}…` : view.backup),
              field("password", "Password", "•".repeat(view.passwordLength)),
              { ...statement, detail: view.confirmed ? "ON" : "OFF" },
              { id: "submit", label: "RESTORE", disabled: !view.ready, message: { _tag: "SubmittedIdentity" } },
            ],
            hints,
          })
        case "Signer":
          return card({
            title: "Connect a signer",
            message: view.problem ?? "Save files and save states stay under the plugin account.",
            answers: [
              field("uri", "Address", view.uri.length > 16 ? `${view.uri.slice(0, 16)}…` : view.uri),
              { ...statement, detail: view.confirmed ? "ON" : "OFF" },
              { id: "submit", label: "CONNECT", disabled: !view.ready, message: { _tag: "SubmittedIdentity" } },
            ],
            hints,
          })
        case "Approving":
          return card({ tone: "tell", title: "Approve in your signer", message: "Your signer app shows a request from this device. Approve it there.", busy: "Waiting for your signer", hints })
      }
      break
    }
    case "Pads": {
      const view = screen.view
      const heard = view.controllers.filter(controller => controller.heard).length
      const seats = view.seats
      const refused = seats !== undefined && typeof seats.state === "object"
      return card({
        tone: view.controllers.length === 0 ? "warn" : "ask",
        title: view.controllers.length === 0 ? "No controllers yet" : heard === 1 ? "1 controller ready" : `${heard} controllers ready`,
        message: view.controllers.length === 0
          ? "Korri sees no controllers yet. Connect one now, or later from its own menu."
          : "Press a button on each controller you will play with. Its lamp lights when Korri hears it.",
        facts: view.controllers.map(controller => ({ label: controller.name, value: controller.heard ? "READY" : "Press any button" })),
        answers: [
          ...(seats === undefined ? [] : [{
            id: "seats",
            label: seats.label,
            detail: seats.control.kind === "step" ? `◀ ${seats.control.current} ▶` : seats.value ?? "",
            ...(seats.control.kind === "step" || seats.control.kind === "cycle"
              ? { message: { _tag: "ChangedSetting", settingId: seats.id, value: seats.control.next } as const }
              : {}),
          }]),
          ...(refused ? [{ id: "ok", label: "OK", message: { _tag: "DismissedSettingsProblem" } as const }] : []),
          { id: "next", label: "NEXT", disabled: !view.ready, message: { _tag: "Continued" } },
        ],
      })
    }
    case "Games": {
      const view = screen.view
      const pack = screen.starterPack
      const packCard = pack._tag === "None" ? undefined : {
        shown: true,
        tone: pack._tag === "Failed" ? "warn" : pack._tag === "Installed" ? "tell" : "ask",
        title: pack._tag === "Installing" ? "Installing Starter pack" : pack._tag === "Installed" ? "Starter pack installed" : pack._tag === "Failed" ? "Starter pack not installed" : "Starter pack",
        message: pack._tag === "Offered" ? "Free games and the programs to play them. No internet needed."
          : pack._tag === "Installing" ? "You can go on. It finishes by itself."
          : pack._tag === "Installed" ? "Its games are in your library."
          : pack.message,
        answers: starterPackChoices(pack).map(choice => ({ id: `pack:${choice.id}`, label: choice.label, disabled: !choice.enabled, message: { _tag: "ChoseStarterPackAction", actionId: choice.id } as const })),
      }
      const scanning = view._tag === "Scanning"
      const hints = scanning && !view.canContinue && (packCard?.answers.length ?? 0) === 0 ? [B("BACK")] : SELECT_BACK
      const answers: Answer[] = scanning
        ? (view.canContinue ? [{ id: "next", label: "NEXT", message: { _tag: "Continued" } }] : [])
        : [
            { id: "next", label: "NEXT", message: { _tag: "Continued" } },
            ...(view.problem === undefined ? [] : [{ id: "ok", label: "OK", message: { _tag: "DismissedSettingsProblem" } as const }]),
            ...(view.canScan ? [{ id: "scan", label: "Scan again", message: { _tag: "ScannedGames" } as const }] : []),
          ]
      const games = view._tag === "Found" ? view.games : []
      const shownGames = games.slice(0, 8)
      return card({
        tone: view._tag === "Failed" ? "warn" : scanning ? "tell" : "ask",
        title: scanning ? "Looking for games" : view._tag === "Found" ? `${games.length} games found` : view._tag === "Empty" ? "No games yet" : "Games not read",
        message: scanning ? "Korri is reading this device's games."
          : view._tag === "Found" ? (games.length === 1 ? "Korri found one game to play." : `Korri found ${games.length} games to play.`)
          : view._tag === "Failed" ? view.message : view.problem ?? "",
        busy: scanning ? "Looking for games" : "",
        carts: shownGames.map(game => cartView(game)),
        more: games.length > shownGames.length ? `+${games.length - shownGames.length} more` : "",
        answers,
        ...(packCard === undefined ? {} : { pack: packCard }),
        hints,
      })
    }
    case "Play":
      return card({
        layout: "done",
        title: screen.finishing ? "Starting up." : "Ready to play.",
        facts: screen.facts,
        ...(screen.finishing ? {} : { go: { id: "finish", label: "GO TO GAMES", message: { _tag: "Finished" } } }),
        hints: screen.finishing ? [] : SELECT_BACK,
      })
  }
  return card({})
}
