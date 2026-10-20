import { STORE_ENTRIES } from "../../fixtures/store"
import { PicoStoreResults } from "./PicoStoreResults"

export const name = "Store results"
export const note = "One provider's search: the SEARCH row, then Korri's results or what stands in for them"

const none = () => undefined
const PROVIDER = "@korri:pico8"

export default function PicoStoreResultsPart() {
  return (
    <PicoStoreResults
      onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS"
      search={{ _tag: "Ready", providerId: PROVIDER, query: "", entries: STORE_ENTRIES }}
    />
  )
}

export function NotSearched() {
  return <PicoStoreResults onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS" search={{ _tag: "Idle" }} />
}

export function Searching() {
  return (
    <PicoStoreResults
      onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS"
      search={{ _tag: "Searching", providerId: "@korri:pico8", query: "celeste" }}
    />
  )
}

export function NoAnswer() {
  return (
    <PicoStoreResults
      onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS"
      search={{
        _tag: "Problem", providerId: "@korri:pico8", query: "celeste",
        message: "The provider did not answer. Check the connection and try again.",
        actions: [{ id: "store:2:search-retry", label: "Retry", enabled: true }],
      }}
    />
  )
}

export function NothingFound() {
  return (
    <PicoStoreResults
      onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS"
      search={{ _tag: "Ready", providerId: "@korri:pico8", query: "zelda", entries: [] }}
    />
  )
}

export function OneResult() {
  return (
    <PicoStoreResults
      onAction={none} onOpen={none} onSearch={none} providerTitle="PICO-8 BBS"
      search={{ _tag: "Ready", providerId: PROVIDER, query: "celeste", entries: STORE_ENTRIES.slice(0, 1) }}
    />
  )
}
