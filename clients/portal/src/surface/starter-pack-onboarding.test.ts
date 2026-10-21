import { expect, test } from "bun:test"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { createStarterPackOnboarding } from "./starter-pack-onboarding"

const PACK = "@korri:starter-pack"
async function packInstall(client: KorridClient) {
  const outcome = await client.pluginRepositories()
  if (outcome._tag !== "Ok") throw new Error("Could not read installed plugins")
  return outcome.payload.otherInstalls.find(install => install.id === PACK)
}
async function settle(check: () => boolean) {
  for (let count = 0; count < 100; count++) { if (check()) return; await Bun.sleep(2) }
  throw new Error("Starter pack did not settle")
}
function gate() {
  let resolve!: () => void
  const promise = new Promise<void>(done => { resolve = done })
  return { promise, resolve }
}

test("offers only a Disabled installed pack and enables before rescanning, once", async () => {
  const base = createInMemoryKorridClient()
  const enable = gate()
  const rescan = gate()
  const calls: unknown[] = []
  const host = createStarterPackOnboarding({ ...base,
    async pluginRepositories() { calls.push(["repositories"]); return base.pluginRepositories() },
    async setPluginEnabled(id, enabled) { calls.push(["enable", id, enabled]); await enable.promise; return base.setPluginEnabled(id, enabled) },
    async rescanDiscovery() { calls.push(["rescan"]); await rescan.promise; return base.rescanDiscovery() },
  })
  expect(host.snapshot()).toBeUndefined()
  host.setNeeded(false)
  expect(calls).toEqual([])
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  expect(host.snapshot()).toEqual({ _tag: "Offered", actions: [{ id: "starter-pack-install", label: "Install", enabled: true }] })
  expect(host.act("unrelated")).toBe(false)
  host.act("starter-pack-install")
  host.act("starter-pack-install")
  expect(host.snapshot()).toEqual({ _tag: "Installing" })
  expect(calls).toEqual([["repositories"], ["enable", PACK, true]])
  enable.resolve()
  await settle(() => calls.length === 3)
  expect(host.snapshot()).toEqual({ _tag: "Installing" })
  rescan.resolve()
  await settle(() => host.snapshot()?._tag === "Installed")
  expect(calls).toEqual([["repositories"], ["enable", PACK, true], ["rescan"]])
  expect(await packInstall(base)).toMatchObject({ id: PACK, enabled: true })
  host.setNeeded(true)
  expect(host.snapshot()).toEqual({ _tag: "Installed" })
  expect(calls.length).toBe(3)
  host.setNeeded(false)
  expect(host.snapshot()).toBeUndefined()
  host.setNeeded(true)
  await Bun.sleep(5)
  expect(host.snapshot()).toBeUndefined() // Enabled when the new setup session began.
})

test("missing, already Enabled, host unavailable and failed status reads make no offer", async () => {
  const base = createInMemoryKorridClient()
  const reads: KorridClient["pluginRepositories"][] = [
    async () => ({ _tag: "Ok", payload: { userAdded: [], repositoryInstalls: [], otherInstalls: [] } }),
    async () => ({ _tag: "Ok", payload: { userAdded: [], repositoryInstalls: [], otherInstalls: [{ id: PACK, enabled: true }] } }),
    async () => ({ _tag: "Err", payload: { code: "PluginHostUnavailable", message: "host detail" } }),
    async () => ({ _tag: "Err", payload: { code: "PluginHostRefused", message: "host detail" } }),
    async () => { throw new Error("host detail") },
  ]
  for (const pluginRepositories of reads) {
    const host = createStarterPackOnboarding({ ...base, pluginRepositories })
    host.setNeeded(true)
    await Bun.sleep(2)
    expect(host.snapshot()).toBeUndefined()
    host.act("starter-pack-install")
    expect(await packInstall(base)).toMatchObject({ id: PACK, enabled: false })
  }
})

test("every refusal and thrown install or rescan failure gives finished copy and a working Retry", async () => {
  for (const failure of ["enable-refused", "enable-threw", "rescan-refused", "rescan-threw"]) {
    const base = createInMemoryKorridClient()
    let refusing = true
    let enables = 0
    const host = createStarterPackOnboarding({ ...base,
      async setPluginEnabled(id, enabled) {
        enables++
        if (refusing && failure === "enable-threw") throw new Error("host detail")
        if (refusing && failure === "enable-refused") return { _tag: "Err", payload: { code: "PluginNeedsReview", message: "host detail" } }
        return base.setPluginEnabled(id, enabled)
      },
      async rescanDiscovery() {
        if (refusing && failure === "rescan-threw") throw new Error("host detail")
        if (refusing && failure === "rescan-refused") return { _tag: "Err", payload: { code: "DiscoveryFailed", message: "host detail" } }
        return base.rescanDiscovery()
      },
    })
    host.setNeeded(true)
    await settle(() => host.snapshot()?._tag === "Offered")
    host.act("starter-pack-install")
    await settle(() => host.snapshot()?._tag === "Failed")
    expect(host.snapshot()).toEqual({ _tag: "Failed", message: "Could not install the Starter pack. Try again.", actions: [{ id: "starter-pack-install", label: "Retry", enabled: true }] })
    refusing = false
    host.act("starter-pack-install")
    await settle(() => host.snapshot()?._tag === "Installed")
    expect(enables).toBe(2)
  }
})

test("finishing setup discards late reads and late install replies", async () => {
  const base = createInMemoryKorridClient()
  const read = gate()
  const enabled = gate()
  let rescans = 0
  const host = createStarterPackOnboarding({ ...base,
    async pluginRepositories() { await read.promise; return base.pluginRepositories() },
    async setPluginEnabled(id, value) { await enabled.promise; return base.setPluginEnabled(id, value) },
    async rescanDiscovery() { rescans++; return base.rescanDiscovery() },
  })
  host.setNeeded(true)
  host.setNeeded(false)
  read.resolve()
  await Bun.sleep(2)
  expect(host.snapshot()).toBeUndefined()
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  host.setNeeded(false)
  enabled.resolve()
  await Bun.sleep(2)
  expect(host.snapshot()).toBeUndefined()
  expect(rescans).toBe(0)
})
