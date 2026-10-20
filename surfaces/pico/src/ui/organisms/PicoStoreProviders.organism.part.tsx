import { STORE_PROVIDERS } from "../../fixtures/store"
import { PicoStoreProviders } from "./PicoStoreProviders"

export const name = "Store providers"
export const note = "Where games can come from: Korri's provider list, its loading, refusal and empty states"

const none = () => undefined

export default function PicoStoreProvidersPart() {
  return <PicoStoreProviders onAction={none} onChoose={none} providers={{ _tag: "Ready", providers: STORE_PROVIDERS }} />
}

export function Loading() {
  return <PicoStoreProviders onAction={none} onChoose={none} providers={{ _tag: "Loading" }} />
}

export function Refused() {
  return (
    <PicoStoreProviders
      onAction={none}
      onChoose={none}
      providers={{ _tag: "Problem", message: "This portal cannot list providers.", actions: [{ id: "store:1:providers-retry", label: "Retry", enabled: true }] }}
    />
  )
}

export function NoneTurnedOn() {
  return <PicoStoreProviders onAction={none} onChoose={none} providers={{ _tag: "Ready", providers: [] }} />
}
