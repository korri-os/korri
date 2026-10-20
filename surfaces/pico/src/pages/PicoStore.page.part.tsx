import { fixtureModel } from "../fixtures/fixture-host"
import { STORE_GETS, storeDetails, storeFixture } from "../fixtures/store"
import { picoTextDraft } from "../pico-text-draft"
import { PicoStore, type PicoStoreShown } from "./PicoStore"

export const name = "Get games"
export const note = "Providers, one provider's search and results, and one entry, with its download on the launch stage"

const none = () => undefined
const typing = { onType: none, onBackspace: none, onClear: none, onToggleCapitals: none, onToggleSymbols: none }
const BBS = "@korri:pico8"

function store(shown: PicoStoreShown, overrides: Parameters<typeof storeFixture>[0] = {}) {
  return (
    <PicoStore
      clockLabel={fixtureModel.clockLabel}
      onAction={none} onChooseProvider={none} onCloseSearch={none} onOpenEntry={none}
      onOpenSearch={none} onPlay={none} onSubmitSearch={none}
      shown={shown}
      store={storeFixture(overrides)}
      typing={typing}
    />
  )
}

export default function PicoStorePagePart() {
  return store({ view: "Providers", providerId: undefined, typing: undefined })
}

export function Results() { return store({ view: "Results", providerId: BBS, typing: undefined }) }
export function Typing() { return store({ view: "Results", providerId: BBS, typing: picoTextDraft("celeste") }) }
export function Entry() { return store({ view: "Entry", providerId: BBS, typing: undefined }) }
export function Confirm() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.confirm) }) }
export function Downloading() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.downloading) }) }
export function DownloadingSizeUnknown() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.unsized) }) }
export function Queued() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.queued) }) }
export function Placing() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.placing) }) }
export function InLibrary() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.inLibrary) }) }
export function Stopped() { return store({ view: "Entry", providerId: BBS, typing: undefined }, { details: storeDetails(STORE_GETS.stopped) }) }
