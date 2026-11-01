/**
 * Getting a game from a provider (PicoStore): the providers, one provider's
 * search and results, then one entry, with where getting it stands in
 * Korri's words and actions. The place is Pico's; every fact is Korri's.
 */
import type { SurfaceAction, SurfaceStore } from "../model"
import { picoStoreProviderTitle, picoStoreResult, picoStoreSearchFor, PICO_STORE_QUERY_MAX } from "../pico-store-view"
import type { PicoStoreShown } from "../pico-types"
import type { Target } from "./focus"
import { frame } from "./frame"
import { A, B, bar, button, card, type CardView, cartView, chip, NO_BAR, NO_CARD } from "./kit"
import { cardTargets, type LaunchStageView } from "./catalog"
import { editorParts } from "./panel"
import { type Act, type Ctx, nav, type Screen } from "./types"

const actionAct = (action: SurfaceAction): Act => [nav({ _tag: "PressedStoreAction", actionId: action.id })]

export function storeScreen(ctx: Ctx, shown: PicoStoreShown, store: SurfaceStore): Screen {
  const focused = ctx.s.focus
  const clock = ctx.korri.clockLabel ?? ""
  const providerTitle = picoStoreProviderTitle(store, shown.providerId)
  const view = shown.view !== "Providers" && providerTitle === undefined && store.providers._tag === "Ready" ? "Providers" : shown.view
  const details = store.details
  const label = view === "Providers" || providerTitle === undefined ? "GET GAMES" : providerTitle.toUpperCase()
  const place = view === "Providers" ? "korri" : "get games"
  const targets: Target<Act>[] = []

  if (shown.typing !== undefined && providerTitle !== undefined) {
    const parts = editorParts({
      group: "SEARCH",
      note: "A name, or part of one",
      prompt: `Search ${providerTitle} for`,
      draft: shown.typing,
      secret: false,
      maxLength: PICO_STORE_QUERY_MAX,
      actions: [
        { label: "SEARCH", act: [nav({ _tag: "SubmittedStoreSearch" })] },
        { label: "CANCEL", act: [nav({ _tag: "ClosedStoreSearch" })] },
      ],
    }, focused)
    return {
      key: "store:typing",
      targets: parts.targets,
      fallback: "key:0:0",
      view: { page: "editor", card: NO_CARD, editor: parts.editor, frame: frame({ place, label, clock, hints: [A("SELECT"), B("CANCEL")] }) },
    }
  }

  const base = (hints: ReturnType<typeof A>[]) => frame({ place, label, clock, hints })
  const empty = { view: "providers", message: "", providers: [] as ReturnType<typeof button>[], search: button("SEARCH"), searchNote: "", results: [] as unknown[], focusedResult: 0, entry: emptyEntry(), inline: NO_CARD }

  if (view === "Providers" || (view === "Entry" && details._tag === "Closed")) {
    const providers = store.providers
    if (providers._tag !== "Ready") {
      const placed = providers._tag === "Loading"
        ? { card: card({ tone: "tell", kicker: "LOOKING", message: "Korri is asking which providers it can use." }), targets: [] }
        : cardTargets(card({ tone: "warn", kicker: "NO PROVIDERS", message: providers.message, actions: providers.actions.map(action => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), providers.actions.map(action => `action:${action.id}`), providers.actions.map(actionAct), focused)
      return { key: `store:providers:${providers._tag}`, targets: placed.targets, view: { page: "store", frame: base([B("BACK")]), card: placed.card, store: empty } }
    }
    const rows = providers.providers.map((provider, i) => {
      const id = `provider:${provider.id}`
      targets.push({ id, box: { x: 0, y: i * 10, w: 100, h: 8 }, act: [nav({ _tag: "ChoseStoreProvider", providerId: provider.id })] })
      return button(provider.title, { focused: focused === id, detail: "▶" })
    })
    return {
      key: "store:providers",
      targets,
      view: {
        page: "store",
        frame: base([A("OPEN"), B("BACK")]),
        card: NO_CARD,
        store: { ...empty, view: "providers", message: rows.length === 0 ? "No provider can get games here yet." : "Pick where to look for a game.", providers: rows },
      },
    }
  }

  if (view === "Results") {
    const search = picoStoreSearchFor(store, shown.providerId)
    targets.push({ id: "search", box: { x: 0, y: 0, w: 100, h: 8 }, act: [nav({ _tag: "OpenedStoreSearch" })] })
    const searchRow = button("SEARCH", { focused: focused === "search", detail: search._tag === "Idle" ? "TYPE A NAME" : search.query })
    if (search._tag === "Problem") {
      const placed = cardTargets(card({ tone: "warn", kicker: "SEARCH FAILED", message: search.message, actions: search.actions.map(action => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), search.actions.map(action => `action:${action.id}`), search.actions.map(actionAct), focused)
      targets.push(...placed.targets.map(target => ({ ...target, box: { ...target.box, y: target.box.y + 20 } })))
      return { key: "store:results:problem", targets, view: { page: "store", frame: base([A("SELECT"), B("BACK")]), card: NO_CARD, store: { ...empty, view: "results", search: searchRow, inline: placed.card } } }
    }
    const entries = search._tag === "Ready" ? search.entries : []
    const results = entries.map((entry, i) => {
      const id = `entry:${entry.id}`
      targets.push({ id, box: { x: i * 10, y: 20, w: 9, h: 20 }, act: [nav({ _tag: "OpenedStoreEntry", entryId: entry.id })] })
      const game = picoStoreResult(entry)
      return { cart: cartView(game, { focused: focused === id, hero: focused === id, band: false }), title: entry.title, subtitle: game.subtitle ?? "" }
    })
    const note = search._tag === "Searching" ? "Searching…"
      : search._tag === "Ready" && entries.length === 0 ? `Nothing matches “${search.query}”.`
      : search._tag === "Idle" ? "Search for a game by its name." : ""
    return {
      key: "store:results",
      targets,
      fallback: results[0] === undefined ? "search" : `entry:${entries[0]?.id ?? ""}`,
      view: {
        page: "store",
        frame: base([A("OPEN"), B("BACK")]),
        card: NO_CARD,
        store: { ...empty, view: "results", search: searchRow, searchNote: note, results, focusedResult: Math.max(0, results.findIndex(result => result.cart.focused)), inline: NO_CARD },
      },
    }
  }

  // An entry.
  if (details._tag === "Closed") return { key: "store:closed", targets: [], view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: empty } }
  const entry = details.entry
  const game = picoStoreResult(entry)
  const cart = cartView(game, { hero: true })
  const entryBase = {
    cart,
    title: entry.title,
    subtitle: [entry.system, providerTitle].filter(part => part !== undefined && part !== "").join(" · "),
    chips: [chip("NOT ON THIS DEVICE", "empty")],
    description: "",
    hasPrimary: false,
    primary: button(""),
    inline: NO_CARD,
    launch: emptyLaunch(),
    working: false,
  }
  if (details._tag === "Loading") {
    return { key: "store:entry:loading", targets: [], view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...entryBase, inline: card({ tone: "tell", kicker: "READING", message: "Korri is asking the provider about this game." }) } } } }
  }
  if (details._tag === "Problem") {
    const placed = cardTargets(card({ tone: "warn", kicker: "COULD NOT READ", message: details.message, actions: details.actions.map(action => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), details.actions.map(action => `action:${action.id}`), details.actions.map(actionAct), focused)
    return { key: "store:entry:problem", targets: placed.targets, view: { page: "store", frame: base([A("SELECT"), B("BACK")]), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...entryBase, inline: placed.card } } } }
  }
  const get = details.get
  const described = { ...entryBase, description: details.description ?? "" }
  let inline: CardView = NO_CARD
  let hints = [B("BACK")]
  switch (get._tag) {
    case "Offer": {
      const [first, ...rest] = get.actions
      if (first !== undefined && first.enabled) targets.push({ id: `action:${first.id}`, box: { x: 0, y: 0, w: 30, h: 10 }, act: actionAct(first) })
      rest.forEach((action, i) => { if (action.enabled) targets.push({ id: `action:${action.id}`, box: { x: 0, y: 20 + i * 10, w: 100, h: 8 }, act: actionAct(action) }) })
      hints = first === undefined ? [B("BACK")] : [A(first.label.toUpperCase()), B("BACK")]
      return {
        key: "store:entry:offer",
        targets,
        view: {
          page: "store",
          frame: base(hints),
          card: NO_CARD,
          store: {
            ...empty,
            view: "entry",
            entry: {
              ...described,
              description: [get.note, described.description].filter(part => part !== undefined && part !== "").join("\n\n"),
              hasPrimary: first !== undefined,
              primary: button((first?.label ?? "").toUpperCase(), { focused: first !== undefined && focused === `action:${first.id}`, disabled: first?.enabled !== true }),
              others: rest.map(action => button(action.label, { disabled: !action.enabled, focused: focused === `action:${action.id}` })),
            },
          },
        },
      }
    }
    case "Unavailable":
      inline = card({ tone: "tell", kicker: "NOT AVAILABLE", message: get.reason })
      break
    case "InLibrary": {
      const playable = get.gameId !== undefined
      const placed = cardTargets(
        card({ tone: "tell", title: "IN YOUR LIBRARY", message: get.detail ?? "", actions: playable ? [button("▶ PLAY")] : [] }),
        ["play"],
        [playable ? [nav({ _tag: "PressedStorePlay", gameId: get.gameId ?? "" })] : undefined],
        focused,
      )
      targets.push(...placed.targets)
      inline = placed.card
      hints = playable ? [A("PLAY"), B("BACK")] : [B("BACK")]
      return { key: "store:entry:library", targets, view: { page: "store", frame: base(hints), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...described, chips: [chip("ON THIS DEVICE", "info")], title: "", subtitle: "", inline } } } }
    }
    case "Confirm":
    case "Problem": {
      const placed = cardTargets(
        card({
          tone: get._tag === "Confirm" ? "ask" : "warn",
          kicker: get.kicker,
          message: get._tag === "Confirm" ? get.detail ?? "" : get.reason,
          actions: get.actions.map(action => button(action.label.toUpperCase(), { disabled: !action.enabled })),
        }),
        get.actions.map(action => `action:${action.id}`),
        get.actions.map(actionAct),
        focused,
      )
      targets.push(...placed.targets)
      inline = placed.card
      hints = [A("SELECT"), B("BACK")]
      break
    }
    case "Working": {
      get.actions.forEach((action, i) => { if (action.enabled) targets.push({ id: `action:${action.id}`, box: { x: i * 20, y: 0, w: 18, h: 8 }, act: actionAct(action) }) })
      const launch: LaunchStageView = {
        kicker: get.kicker,
        detail: get.detail ?? "",
        bar: get.progress === undefined ? { ...NO_BAR, shown: true, walking: true } : bar(get.progress),
        hasCart: true,
        cart,
        phase: "starting",
        actions: get.actions.map(action => button(action.label.toUpperCase(), { disabled: !action.enabled, focused: focused === `action:${action.id}` })),
      }
      return { key: "store:entry:working", targets, view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: { ...empty, view: "working", entry: { ...described, launch, working: true } } } }
    }
  }
  return { key: `store:entry:${get._tag}`, targets, view: { page: "store", frame: base(hints), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...described, inline } } } }
}

function emptyLaunch(): LaunchStageView {
  return { kicker: "", detail: "", bar: NO_BAR, hasCart: false, cart: cartView({ id: "", title: "" }), phase: "starting", actions: [] }
}

function emptyEntry() {
  return {
    cart: cartView({ id: "", title: "" }),
    title: "",
    subtitle: "",
    chips: [],
    description: "",
    hasPrimary: false,
    primary: button(""),
    others: [],
    inline: NO_CARD,
    launch: emptyLaunch(),
    working: false,
  }
}
