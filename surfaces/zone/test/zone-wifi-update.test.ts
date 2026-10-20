/**
 * The Wi-Fi rules, one block per function, driven with plain values.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceWifiAttempt } from "@contracts/surface/korri-surface"
import {
  fixtureNetworkConnected,
  fixtureNetworkConnecting,
  fixtureNetworkNoAdapter,
  fixtureNetworkRejected,
  fixtureNetworkScanning,
} from "../src/fixtures/fixture-options"
import { zoneBars, zoneConnectionValue, zoneNetworkValue } from "../src/plan/options"
import { joinHidden, lastAttempt, pickNetwork, rejectedEntry, sameAttempt, scan, scannedNetworks, submitWifi } from "../src/state/wifi"

const connected = { network: fixtureNetworkConnected }

describe("scannedNetworks and lastAttempt", () => {
  test("read Korri's scan and attempt, or none", () => {
    expect(scannedNetworks(connected)).toHaveLength(4)
    expect(scannedNetworks({ network: fixtureNetworkNoAdapter })).toEqual([])
    expect(lastAttempt({})).toEqual({ _tag: "Idle" })
    expect(lastAttempt({ network: fixtureNetworkConnecting })).toEqual({ _tag: "Connecting", ssid: "Attic 5G" })
  })
})

describe("pickNetwork", () => {
  test("a known network joins with no password", () => {
    const known = { network: { ...fixtureNetworkConnected, connection: { _tag: "Offline" as const } } }
    expect(pickNetwork(known, "ap-hummingbird")).toEqual({ _tag: "Send", request: { _tag: "ConnectWifi", networkId: "ap-hummingbird" } })
  })
  test("an open network joins with no password", () => {
    expect(pickNetwork(connected, "ap-cafe")).toEqual({ _tag: "Send", request: { _tag: "ConnectWifi", networkId: "ap-cafe" } })
  })
  test("a secured network Korri does not know asks its password", () => {
    expect(pickNetwork(connected, "ap-attic"))
      .toEqual({ _tag: "Type", entry: { _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" } })
  })
  test("the network in use, an unknown id, or any while joining does nothing", () => {
    expect(pickNetwork(connected, "ap-hummingbird")).toEqual({ _tag: "Nothing" })
    expect(pickNetwork(connected, "nope")).toEqual({ _tag: "Nothing" })
    expect(pickNetwork({ network: fixtureNetworkConnecting }, "ap-cafe")).toEqual({ _tag: "Nothing" })
  })
})

describe("scan", () => {
  test("asks once, not while a scan runs, not without an adapter", () => {
    expect(scan(connected)).toEqual({ _tag: "Send", request: { _tag: "ScanWifi" } })
    expect(scan({ network: fixtureNetworkScanning })).toEqual({ _tag: "Nothing" })
    expect(scan({ network: fixtureNetworkNoAdapter })).toEqual({ _tag: "Nothing" })
  })
})

describe("joinHidden and submitWifi", () => {
  test("a hidden network asks its name, then its password", () => {
    expect(joinHidden()).toEqual({ _tag: "Type", entry: { _tag: "HiddenSsid" } })
    expect(submitWifi({ _tag: "HiddenSsid" }, "  attic-ap ")).toEqual({ _tag: "Type", entry: { _tag: "HiddenPassword", ssid: "attic-ap" } })
    expect(submitWifi({ _tag: "HiddenSsid" }, "   ")).toEqual({ _tag: "Nothing" })
  })
  test("a hidden network's password may be empty: an open network", () => {
    expect(submitWifi({ _tag: "HiddenPassword", ssid: "attic-ap" }, ""))
      .toEqual({ _tag: "Send", request: { _tag: "ConnectHiddenWifi", ssid: "attic-ap" } })
    expect(submitWifi({ _tag: "HiddenPassword", ssid: "attic-ap" }, "k"))
      .toEqual({ _tag: "Send", request: { _tag: "ConnectHiddenWifi", ssid: "attic-ap", password: "k" } })
  })
  test("a listed network's password may not be empty", () => {
    expect(submitWifi({ _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" }, "")).toEqual({ _tag: "Nothing" })
  })
})

describe("rejectedEntry", () => {
  const connecting: SurfaceWifiAttempt = { _tag: "Connecting", ssid: "Attic 5G" }
  test("a fresh rejection reopens that network's password", () => {
    expect(rejectedEntry(connecting, { network: fixtureNetworkRejected }))
      .toEqual({ _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" })
  })
  test("the same rejection again, or a failure that is not the key, reopens nothing", () => {
    expect(rejectedEntry(lastAttempt({ network: fixtureNetworkRejected }), { network: fixtureNetworkRejected })).toBeUndefined()
    const other = fixtureNetworkRejected.wifi._tag === "Available"
      ? { ...fixtureNetworkRejected, wifi: { ...fixtureNetworkRejected.wifi, attempt: { _tag: "Failed" as const, ssid: "Attic 5G", message: "Out of range", passwordRejected: false } } }
      : fixtureNetworkRejected
    expect(rejectedEntry(connecting, { network: other })).toBeUndefined()
  })
  test("a hidden network is asked by its name", () => {
    const hidden = fixtureNetworkRejected.wifi._tag === "Available"
      ? { ...fixtureNetworkRejected, wifi: { ...fixtureNetworkRejected.wifi, attempt: { _tag: "Failed" as const, ssid: "attic-ap", message: "No.", passwordRejected: true } } }
      : fixtureNetworkRejected
    expect(rejectedEntry(connecting, { network: hidden })).toEqual({ _tag: "HiddenPassword", ssid: "attic-ap" })
  })
})

describe("sameAttempt", () => {
  test("compares fields, not identity", () => {
    expect(sameAttempt({ _tag: "Idle" }, { _tag: "Idle" })).toBe(true)
    expect(sameAttempt({ _tag: "Connecting", ssid: "a" }, { _tag: "Connecting", ssid: "b" })).toBe(false)
    expect(sameAttempt(
      { _tag: "Failed", ssid: "a", message: "m", passwordRejected: true },
      { _tag: "Failed", ssid: "a", message: "m", passwordRejected: true },
    )).toBe(true)
  })
})

describe("what a row reads", () => {
  test("bars: one lit per quarter, never none", () => {
    expect(zoneBars(0)).toBe("▂▁▁▁")
    expect(zoneBars(72)).toBe("▂▄▆▁")
    expect(zoneBars(100)).toBe("▂▄▆█")
  })
  test("the connection: SSID, bars and strength; WIRED; OFFLINE", () => {
    expect(zoneConnectionValue({ _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 })).toBe("hummingbird-lake ▂▄▆▁ 72%")
    expect(zoneConnectionValue({ _tag: "Wired" })).toBe("WIRED")
    expect(zoneConnectionValue({ _tag: "Offline" })).toBe("OFFLINE")
  })
  test("a network: bars, strength, LOCK when secured, KNOWN when known", () => {
    expect(zoneNetworkValue({ id: "a", ssid: "a", strengthPercent: 72, secured: true, known: true })).toBe("▂▄▆▁ 72% LOCK KNOWN")
    expect(zoneNetworkValue({ id: "b", ssid: "b", strengthPercent: 48, secured: false, known: false })).toBe("▂▄▁▁ 48%")
  })
})
