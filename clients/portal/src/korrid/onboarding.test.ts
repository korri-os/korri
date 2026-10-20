import { describe, expect, test } from "bun:test"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

describe("first boot through the in-memory korrid contract", () => {
  test("prototype scan, accepted/rejected passwords, hidden activation and finish", async () => {
    const client = createInMemoryKorridClient()
    expect(await client.onboardingStatus()).toEqual({ _tag: "Ok", payload: { needed: true } })
    const scan = await client.scanWifi()
    if (scan._tag !== "Ok" || scan.payload.wifi._tag !== "Available") throw new Error("scan failed")
    const networks = scan.payload.wifi.payload.networks
    expect(networks.map(network => network.ssid)).toEqual(["Home-5G", "Home-2.4", "NETGEAR-7B21", "Corner Cafe Free", "DIRECT-4F-LaserJet"])
    for (const network of networks) {
      const outcome = await client.connectWifi(network.id, "anything")
      if (outcome._tag !== "Ok" || outcome.payload.wifi._tag !== "Available") throw new Error("activation failed")
      const rejects = network.ssid === "NETGEAR-7B21" || network.ssid === "DIRECT-4F-LaserJet"
      expect(outcome.payload.wifi.payload.attempt._tag).toBe(rejects ? "Failed" : "Idle")
      if (outcome.payload.wifi.payload.attempt._tag === "Failed") expect(outcome.payload.wifi.payload.attempt.payload.passwordRejected).toBe(true)
      expect(JSON.stringify(outcome)).not.toContain("anything")
    }
    expect((await client.connectHiddenWifi("Hidden", "anything"))._tag).toBe("Ok")
    expect(await client.finishOnboarding()).toEqual({ _tag: "Ok", payload: { needed: false } })
    expect(await client.onboardingStatus()).toEqual({ _tag: "Ok", payload: { needed: false } })
    expect(await client.restartOnboarding()).toEqual({ _tag: "Ok", payload: { needed: true } })
    expect(await client.onboardingStatus()).toEqual({ _tag: "Ok", payload: { needed: true } })
  })
  test("transport failures cannot put a password in error text", async () => {
    const client = createHttpKorridClient("http://127.0.0.1:1", "test")
    const outcome = await client.connectWifi("ap", "test-passphrase")
    expect(outcome._tag).toBe("Err")
    expect(JSON.stringify(outcome)).not.toContain("test-passphrase")
  })
})

test("HTTP network commands carry the password only in the tagged RPC body", async () => {
  const snapshot = await createInMemoryKorridClient().networkStatus()
  let received = 0
  const server = Bun.serve({
    hostname: "127.0.0.1", port: 0,
    async fetch(request) {
      expect(new URL(request.url).pathname).toBe("/rpc")
      expect(new URL(request.url).search).toBe("")
      expect(request.url).not.toContain("test-passphrase")
      const body = await request.json() as { _tag: string; payload: { password?: string } }
      expect(body.payload.password).toBe("test-passphrase")
      received++
      return Response.json({ _tag: body._tag, outcome: snapshot })
    },
  })
  try {
    const client = createHttpKorridClient(`http://127.0.0.1:${server.port}`, "test-capability")
    expect((await client.connectWifi("ap", "test-passphrase"))._tag).toBe("Ok")
    expect((await client.connectHiddenWifi("Hidden", "test-passphrase"))._tag).toBe("Ok")
    expect(received).toBe(2)
  } finally { await server.stop(true) }
})
