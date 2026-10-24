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
  for (let count = 0; count < 500; count++) { if (check()) return; await Bun.sleep(2) }
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
    async rescanLibraryFolders() { calls.push(["rescan"]); await rescan.promise; return base.rescanLibraryFolders() },
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

test("missing, already Enabled, host unavailable and failed repository reads make no offer", async () => {
  const base = createInMemoryKorridClient()
  const reads: KorridClient["pluginRepositories"][] = [
    async () => ({ _tag: "Ok", payload: { userAdded: [], repositoryInstalls: [], otherInstalls: [] } }),
    async () => ({ _tag: "Ok", payload: { userAdded: [], repositoryInstalls: [{ id: PACK, sourceUrl: "https://example.com/catalog.json", releaseVersion: "1", enabled: false }], otherInstalls: [] } }),
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
      async rescanLibraryFolders() {
        if (refusing && failure === "rescan-threw") throw new Error("host detail")
        if (refusing && failure === "rescan-refused") return { _tag: "Err", payload: { code: "DiscoveryFailed", message: "host detail" } }
        return base.rescanLibraryFolders()
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
    expect(enables).toBe(failure.startsWith("enable-") ? 2 : 1)
  }
})

test("finishing setup hides late replies but an accepted install still rescans", async () => {
  const base = createInMemoryKorridClient()
  const read = gate()
  const enabled = gate()
  let rescans = 0
  let discoveries = 0
  let completed = false
  const host = createStarterPackOnboarding({ ...base,
    async pluginRepositories() { await read.promise; return base.pluginRepositories() },
    async setPluginEnabled(id, value) { await enabled.promise; return base.setPluginEnabled(id, value) },
    async rescanLibraryFolders() { rescans++; return base.rescanLibraryFolders() },
    async libraryFoldersSnapshot() { discoveries++; return base.libraryFoldersSnapshot() },
  }, { pollIntervalMs: 1, onInstalled: () => { completed = true } })
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
  await settle(() => completed)
  expect(host.snapshot()).toBeUndefined()
  expect(rescans).toBe(1)
  expect(discoveries).toBeGreaterThan(0)
})


test("Installing lasts until discovery finishes, not just until the rescan is accepted", async () => {
  const base = createInMemoryKorridClient()
  const finish = gate()
  let reads = 0
  const host = createStarterPackOnboarding({ ...base,
    async libraryFoldersSnapshot() { reads++; await finish.promise; return base.libraryFoldersSnapshot() },
  })
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await Bun.sleep(5)
  expect(host.snapshot()?._tag).toBe("Installing")
  finish.resolve()
  await settle(() => host.snapshot()?._tag === "Installed")
  expect(reads).toBeGreaterThan(0)
})

test("a discovery Problem fails installation and Retry does not enable the pack again", async () => {
  const base = createInMemoryKorridClient({ discoveryScan: { delayMs: 5, results: [
    { _tag: "Problem", payload: {} }, { _tag: "Idle", payload: {} },
  ] } })
  let enables = 0
  let rescans = 0
  const host = createStarterPackOnboarding({ ...base,
    async setPluginEnabled(id, value) { enables++; return base.setPluginEnabled(id, value) },
    async rescanLibraryFolders() { rescans++; return base.rescanLibraryFolders() },
  })
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag !== "Installing")
  expect(host.snapshot()).toMatchObject({ _tag: "Failed", message: "Could not install the Starter pack. Try again." })
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Installed")
  expect(enables).toBe(1)
  expect(rescans).toBe(2)
})


test("the discovery deadline bounds a scan that never finishes and Retry only rescans", async () => {
  const base = createInMemoryKorridClient({ discoveryScan: { results: [
    { _tag: "Scanning", payload: {} }, { _tag: "Idle", payload: {} },
  ] } })
  let reads = 0
  let enables = 0
  const host = createStarterPackOnboarding({ ...base,
    async setPluginEnabled(id, value) { enables++; return base.setPluginEnabled(id, value) },
    async libraryFoldersSnapshot() { reads++; return base.libraryFoldersSnapshot() },
  }, { pollIntervalMs: 1, discoveryTimeoutMs: 20 })
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Failed")
  const stopped = reads
  await Bun.sleep(10)
  expect(reads).toBe(stopped)
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Installed")
  expect(enables).toBe(1)
})

test("a stuck discovery snapshot is bounded and its late reply cannot change Failed", async () => {
  const base = createInMemoryKorridClient()
  const reply = gate()
  let reads = 0
  const host = createStarterPackOnboarding({ ...base,
    async libraryFoldersSnapshot() { reads++; await reply.promise; return base.libraryFoldersSnapshot() },
  }, { pollIntervalMs: 1, discoveryTimeoutMs: 15 })
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Failed")
  expect(reads).toBe(1)
  reply.resolve()
  await Bun.sleep(5)
  expect(host.snapshot()?._tag).toBe("Failed")
})

test("unmount stops polling without publishing late model updates", async () => {
  const base = createInMemoryKorridClient({ discoveryScan: { results: [{ _tag: "Enriching", payload: {} }] } })
  let reads = 0
  let completed = false
  const host = createStarterPackOnboarding({ ...base,
    async libraryFoldersSnapshot() { reads++; return base.libraryFoldersSnapshot() },
  }, { pollIntervalMs: 1, discoveryTimeoutMs: 50, onInstalled: () => { completed = true } })
  const stop = host.start()
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await settle(() => reads >= 2)
  expect(host.snapshot()?._tag).toBe("Installing")
  stop()
  const stopped = reads
  await Bun.sleep(60)
  expect(reads).toBe(stopped)
  expect(host.snapshot()).toBeUndefined()
  expect(completed).toBe(false)
})

test("unmount during enable still dispatches the accepted rescan but starts no polling", async () => {
  const base = createInMemoryKorridClient()
  const enabled = gate()
  let rescans = 0
  let reads = 0
  const host = createStarterPackOnboarding({ ...base,
    async setPluginEnabled(id, value) { await enabled.promise; return base.setPluginEnabled(id, value) },
    async rescanLibraryFolders() { rescans++; return base.rescanLibraryFolders() },
    async libraryFoldersSnapshot() { reads++; return base.libraryFoldersSnapshot() },
  })
  const stop = host.start()
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  stop()
  enabled.resolve()
  await settle(() => rescans === 1)
  await Bun.sleep(5)
  expect(reads).toBe(0)
  expect(host.snapshot()).toBeUndefined()
})

test("Retry reads otherInstalls when an enable reply was lost after commit", async () => {
  const base = createInMemoryKorridClient()
  let enables = 0
  let reads = 0
  const host = createStarterPackOnboarding({ ...base,
    async pluginRepositories() { reads++; return base.pluginRepositories() },
    async setPluginEnabled(id, value) { enables++; await base.setPluginEnabled(id, value); throw new Error("lost reply") },
  }, { pollIntervalMs: 1 })
  host.setNeeded(true)
  await settle(() => host.snapshot()?._tag === "Offered")
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Failed")
  host.act("starter-pack-install")
  await settle(() => host.snapshot()?._tag === "Installed")
  expect(enables).toBe(1)
  expect(reads).toBe(2)
})
