import { describe, expect, test } from "bun:test"
import { createInMemoryKorridClient } from "../korrid/client"
import { createNetworkOnboarding, networkFrom } from "./network-onboarding"

async function waitFor(check: () => boolean) {
  for (let count = 0; count < 100; count++) { if (check()) return; await Bun.sleep(5) }
  throw new Error("Model did not settle")
}
describe("network onboarding host", () => {
  test("publishes Connecting before the result, excludes passwords, finishes only after acknowledgement", async () => {
    const host = createNetworkOnboarding(createInMemoryKorridClient())
    await host.refresh()
    expect(host.snapshot().onboardingNeeded).toBe(true)
    const models: string[] = []
    host.subscribe(() => models.push(JSON.stringify(host.snapshot())))
    host.connectWifi("netgear", "test-passphrase")
    const wifi = host.snapshot().network?.wifi
    expect(wifi?._tag === "Available" && wifi.attempt).toEqual({ _tag: "Connecting", ssid: "NETGEAR-7B21" })
    await waitFor(() => { const wifi = host.snapshot().network?.wifi; return wifi?._tag === "Available" && wifi.attempt._tag === "Failed" })
    const failed = host.snapshot().network?.wifi
    expect(failed?._tag === "Available" && failed.attempt).toMatchObject({ _tag: "Failed", passwordRejected: true })
    host.connectHiddenWifi("Hidden", "test-passphrase")
    await waitFor(() => host.snapshot().network?.connection._tag === "Wifi")
    host.finishOnboarding()
    expect(host.snapshot().onboardingNeeded).toBe(true)
    await waitFor(() => host.snapshot().onboardingNeeded === false)
    expect(models.join()).not.toContain("test-passphrase")
  })
  test("late reads cannot overwrite an active command, and duplicate commands do not dispatch", async () => {
    const client = createInMemoryKorridClient()
    const host = createNetworkOnboarding(client)
    await host.refresh()
    host.connectWifi("home-5g", "test-passphrase")
    host.connectWifi("printer", "test-passphrase")
    await host.refresh()
    const wifi = host.snapshot().network?.wifi
    expect(wifi?._tag === "Available" && wifi.attempt).toEqual({ _tag: "Connecting", ssid: "Home-5G" })
    await waitFor(() => host.snapshot().network?.connection._tag === "Wifi")
    expect(host.snapshot().network?.connection).toMatchObject({ _tag: "Wifi", ssid: "Home-5G" })
  })
  test("failed finish keeps setup visible, and error text never echoes a dependency error", async () => {
    const client = createInMemoryKorridClient()
    const host = createNetworkOnboarding({ ...client,
      async finishOnboarding() { throw new Error("test-passphrase") },
      async connectWifi() { throw new Error("test-passphrase") },
    })
    await host.refresh()
    host.connectWifi("home-5g", "test-passphrase")
    await waitFor(() => host.snapshot().problem !== undefined)
    expect(JSON.stringify(host.snapshot())).not.toContain("test-passphrase")
    host.finishOnboarding()
    await Bun.sleep(5)
    expect(host.snapshot().onboardingNeeded).toBe(true)
    expect(host.snapshot().problem).toContain("Try again")
  })
  test("NoAdapter, Wired, scan facts and failure copy map without wire tags leaking", () => {
    expect(networkFrom({ connection: { _tag: "Wired" }, wifi: { _tag: "NoAdapter" } })).toEqual({ connection: { _tag: "Wired" }, wifi: { _tag: "NoAdapter" } })
  })
})

test("a slow status read is not restarted or starved by a second refresh", async () => {
  const client = createInMemoryKorridClient()
  let complete!: () => void
  const gate = new Promise<void>(resolve => { complete = resolve })
  let calls = 0
  const host = createNetworkOnboarding({ ...client, async networkStatus() { calls++; await gate; return client.networkStatus() } })
  const first = host.refresh()
  await host.refresh()
  expect(calls).toBe(1)
  complete()
  await first
  expect(host.snapshot().network).toBeDefined()
})

test("restarting setup brings onboarding back, and a failed restart says so", async () => {
  const client = createInMemoryKorridClient()
  const host = createNetworkOnboarding(client)
  await host.refresh()
  host.finishOnboarding()
  await waitFor(() => host.snapshot().onboardingNeeded === false)
  host.restartOnboarding()
  await waitFor(() => host.snapshot().onboardingNeeded === true)
  expect(host.snapshot().problem).toBeUndefined()
  // Setup can be finished again afterwards.
  host.finishOnboarding()
  await waitFor(() => host.snapshot().onboardingNeeded === false)

  const failing = createNetworkOnboarding({ ...client,
    async restartOnboarding() { return { _tag: "Err", payload: { code: "OnboardingStorageUnavailable", message: "dependency text" } } },
  })
  await failing.refresh()
  expect(failing.snapshot().onboardingNeeded).toBe(false)
  failing.restartOnboarding()
  await waitFor(() => failing.snapshot().problem !== undefined)
  expect(failing.snapshot().onboardingNeeded).toBe(false)
  expect(failing.snapshot().problem).toBe("Could not restart setup. Try again.")
})

test("restart rejects duplicate writes and a stale status cannot undo it", async () => {
  const client = createInMemoryKorridClient()
  await client.finishOnboarding()
  let releaseRead!: () => void
  let releaseRestart!: () => void
  const readGate = new Promise<void>(resolve => { releaseRead = resolve })
  const restartGate = new Promise<void>(resolve => { releaseRestart = resolve })
  let delayRead = false
  let restarts = 0
  const host = createNetworkOnboarding({ ...client,
    async onboardingStatus() {
      const snapshot = await client.onboardingStatus()
      if (delayRead) await readGate
      return snapshot
    },
    async restartOnboarding() {
      restarts++
      await restartGate
      return client.restartOnboarding()
    },
  })
  await host.refresh()
  delayRead = true
  const staleRead = host.refresh()
  host.restartOnboarding()
  host.restartOnboarding()
  host.finishOnboarding()
  expect(restarts).toBe(1)
  expect(host.snapshot().onboardingNeeded).toBe(false)
  releaseRestart()
  await waitFor(() => host.snapshot().onboardingNeeded === true)
  releaseRead()
  await staleRead
  expect(host.snapshot().onboardingNeeded).toBe(true)
  expect(await client.onboardingStatus()).toEqual({ _tag: "Ok", payload: { needed: true } })
})

test("finishing setup is not dropped while a connection is in flight", async () => {
  const host = createNetworkOnboarding(createInMemoryKorridClient())
  await host.refresh()
  host.connectWifi("home-5g", "test-passphrase")
  host.finishOnboarding()
  await waitFor(() => host.snapshot().onboardingNeeded === false)
  await waitFor(() => host.snapshot().network?.connection._tag === "Wifi")
  expect(host.snapshot().onboardingNeeded).toBe(false)
})
