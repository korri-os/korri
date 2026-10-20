import type { SurfaceStore } from "@contracts/surface/korri-surface"
import { PICO_STORE_QUERY_MAX, picoStoreCart, picoStoreProviderTitle, picoStoreSearchFor } from "../pico-store-view"
import type { PicoTextDraft } from "../pico-text-draft"
import type { PicoTyping } from "../ui/molecules/PicoKeyboard"
import { PicoLaunchStage } from "../ui/organisms/PicoLaunchStage"
import { PicoStoreEntry } from "../ui/organisms/PicoStoreEntry"
import { PicoStoreProviders } from "../ui/organisms/PicoStoreProviders"
import { PicoStoreResults } from "../ui/organisms/PicoStoreResults"
import { PicoTextEditor } from "../ui/organisms/PicoTextEditor"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

/** Which store page shows, and what is being typed while the search
 * keyboard is open. Pico's place, never Korri's fact. */
export interface PicoStoreShown {
  readonly view: "Providers" | "Results" | "Entry"
  readonly providerId: string | undefined
  readonly typing: PicoTextDraft | undefined
}

/**
 * Getting a game from a provider, in the same shell as everything else:
 * the providers, then one provider's search and results, then one entry.
 *
 * Search types on the keyboard Wi-Fi passwords use (PicoTextEditor), with
 * SEARCH and CANCEL beside the keys. An entry being downloaded takes the
 * launch stage: the cart goes into the slot while Korri's bytes fill the
 * sixteen cells, as a first launch's download does. Back walks one page at a
 * time and never cancels: a download keeps going, and shows again here.
 */
export function PicoStore({
  store,
  shown,
  clockLabel,
  typing,
  onChooseProvider,
  onOpenSearch,
  onSubmitSearch,
  onCloseSearch,
  onOpenEntry,
  onAction,
  onPlay,
}: {
  readonly store: SurfaceStore
  readonly shown: PicoStoreShown
  readonly clockLabel?: string
  readonly typing: PicoTyping
  readonly onChooseProvider: (providerId: string) => void
  readonly onOpenSearch: () => void
  readonly onSubmitSearch: () => void
  readonly onCloseSearch: () => void
  readonly onOpenEntry: (entryId: string) => void
  readonly onAction: (actionId: string) => void
  readonly onPlay: (gameId: string) => void
}) {
  const providerTitle = picoStoreProviderTitle(store, shown.providerId)
  // A provider Korri stopped listing cannot hold its pages.
  const view = shown.view !== "Providers" && providerTitle === undefined && store.providers._tag === "Ready" ? "Providers" : shown.view
  const details = store.details
  const working = view === "Entry" && details._tag === "Ready" && details.get._tag === "Working" ? details.get : undefined
  const primary = view === "Entry" && details._tag === "Ready" && "actions" in details.get
    ? details.get.actions.find(action => action.enabled)?.label.toUpperCase()
    : view === "Entry" && details._tag === "Ready" && details.get._tag === "InLibrary" && details.get.gameId !== undefined ? "PLAY"
      : view === "Entry" ? undefined : "OPEN"
  const hints = shown.typing !== undefined
    ? [{ hintKey: "a" as const, label: "SELECT" }, { hintKey: "b" as const, label: "CANCEL" }]
    : [...(primary === undefined ? [] : [{ hintKey: "a" as const, label: primary }]), { hintKey: "b" as const, label: "BACK" }]
  return (
    <PicoScreenShell
      clockLabel={clockLabel}
      hints={hints}
      label={view === "Providers" || providerTitle === undefined ? "GET GAMES" : providerTitle.toUpperCase()}
      place={view === "Providers" ? "korri" : "get games"}
    >
      {shown.typing !== undefined && providerTitle !== undefined ? (
        <PicoTextEditor
          actions={[
            { label: "SEARCH", onPress: onSubmitSearch },
            { label: "CANCEL", onPress: onCloseSearch },
          ]}
          draft={shown.typing}
          formLabel={`Search ${providerTitle}`}
          label="Search"
          maxLength={PICO_STORE_QUERY_MAX}
          note="A name, or part of one"
          prompt={`Search ${providerTitle} for`}
          typing={typing}
        />
      ) : view === "Providers" ? (
        <PicoStoreProviders onAction={onAction} onChoose={onChooseProvider} providers={store.providers} />
      ) : view === "Results" ? (
        <PicoStoreResults
          onAction={onAction}
          onOpen={onOpenEntry}
          onSearch={onOpenSearch}
          providerTitle={providerTitle ?? ""}
          search={picoStoreSearchFor(store, shown.providerId)}
        />
      ) : details._tag === "Closed" ? (
        <PicoStoreProviders onAction={onAction} onChoose={onChooseProvider} providers={store.providers} />
      ) : working !== undefined ? (
        <PicoLaunchStage
          actions={working.actions.map(action => ({ id: action.id, label: action.label, disabled: !action.enabled, onPress: () => onAction(action.id) }))}
          cart={picoStoreCart(details.entry)}
          detail={working.detail}
          gameTitle={details.entry.title}
          kicker={working.kicker}
          // Waiting in korrid's queue or for the provider has no position: the
          // band walks rather than the clock filling, so nothing looks like a bar.
          progress={working.progress ?? { done: 0 }}
        />
      ) : (
        <PicoStoreEntry details={details} onAction={onAction} onPlay={onPlay} providerTitle={providerTitle} />
      )}
    </PicoScreenShell>
  )
}
