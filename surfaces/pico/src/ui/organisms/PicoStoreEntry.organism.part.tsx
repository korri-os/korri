import { STORE_ENTRIES, STORE_GETS, storeDetails } from "../../fixtures/store"
import { PicoStoreEntry } from "./PicoStoreEntry"

export const name = "Store entry"
export const note = "One entry and where getting it stands, in Korri's words and with Korri's actions"

const none = () => undefined
const entry = (get: Parameters<typeof storeDetails>[0], at = 0) => (
  <PicoStoreEntry details={storeDetails(get, STORE_ENTRIES[at])} onAction={none} onPlay={none} providerTitle="PICO-8 BBS" />
)

export default function PicoStoreEntryPart() {
  return entry(STORE_GETS.offer)
}

export function Cancelled() { return entry(STORE_GETS.cancelled) }
export function SizeUnknown() { return entry(STORE_GETS.confirm) }
export function SystemNotPlayable() { return entry(STORE_GETS.unavailable) }
export function InLibrary() { return entry(STORE_GETS.inLibrary) }
export function NotRecognised() { return entry(STORE_GETS.staged) }
export function Failed() { return entry(STORE_GETS.failed) }
export function Stopped() { return entry(STORE_GETS.stopped) }
export function LongTitle() { return entry(STORE_GETS.offer, 1) }
export function NoSystem() { return entry({ _tag: "Unavailable", reason: "This provider doesn't say which system this game is for, so it can't be downloaded yet." }, 2) }

export function Reading() {
  return <PicoStoreEntry details={{ _tag: "Loading", entry: STORE_ENTRIES[0]! }} onAction={none} onPlay={none} providerTitle="PICO-8 BBS" />
}

export function NoAnswer() {
  return (
    <PicoStoreEntry
      details={{ _tag: "Problem", entry: STORE_ENTRIES[0]!, message: "The provider did not answer.", actions: [{ id: "store:9:details-retry", label: "Retry", enabled: true }] }}
      onAction={none} onPlay={none} providerTitle="PICO-8 BBS"
    />
  )
}
