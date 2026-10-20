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
import { zoneConnectionValue, zoneNetworkValue } from "../src/plan/options"
import { zoneBarsLit } from "../src/ui/atoms/ZoneBars"
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
  test("bars: one lit per quarter, never none while connected, none offline", () => {
    expect(zoneBarsLit(0)).toBe(1)
    expect(zoneBarsLit(72)).toBe(3)
    expect(zoneBarsLit(100)).toBe(4)
    expect(zoneBarsLit(undefined)).toBe(1)
    expect(zoneBarsLit(80, false)).toBe(0)
  })
  test("the connection's words: SSID and strength; WIRED; OFFLINE. No glyphs: the row draws bars", () => {
    expect(zoneConnectionValue({ _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 })).toBe("hummingbird-lake 72%")
    expect(zoneConnectionValue({ _tag: "Wired" })).toBe("WIRED")
    expect(zoneConnectionValue({ _tag: "Offline" })).toBe("OFFLINE")
  })
  test("a network's words: strength, LOCKED or OPEN, SAVED when known; one set for OPTIONS and first boot", () => {
    expect(zoneNetworkValue({ id: "a", ssid: "a", strengthPercent: 72, secured: true, known: true })).toBe("72% LOCKED SAVED")
    expect(zoneNetworkValue({ id: "b", ssid: "b", strengthPercent: 48, secured: false, known: false })).toBe("48% OPEN")
    expect(zoneNetworkValue({ id: "c", ssid: "c", strengthPercent: 21, secured: true, known: false })).toBe("21% LOCKED")
    expect(zoneNetworkValue({ id: "d", ssid: "d", strengthPercent: 90, secured: false, known: true })).toBe("90% OPEN SAVED")
  })
})
