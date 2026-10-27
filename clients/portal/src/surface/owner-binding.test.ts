import { expect, test } from "bun:test"
import { OwnerSignerKind, type OwnerBindingSnapshot } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "../korrid/client"
import { createOwnerBindingController, ownerBindingStatus, OWNER_BINDING_ACTION, OWNER_BINDING_CONTINUE } from "./owner-binding"

const unowned: OwnerBindingSnapshot = {
  identity: { _tag: "Unowned", devicePublicKey: "22".repeat(32) },
  personSigner: { _tag: OwnerSignerKind.Unavailable, message: "Choose a signer" },
  deviceFingerprint: "22".repeat(32), requestedAction: "Bind this device to your person identity",
}
const owned: OwnerBindingSnapshot = {
  ...unowned,
  identity: { _tag: "Owned", devicePublicKey: unowned.deviceFingerprint,
    ownerPublicKey: "11".repeat(32), eventId: "33".repeat(32), createdAt: 1 },
  personSigner: { _tag: OwnerSignerKind.Approved, message: "Approved" },
}
async function waitFor(check: () => boolean) {
  for (let i = 0; i < 100; ++i) { if (check()) return; await Bun.sleep(5) }
  throw new Error("Owner setup did not settle")
}

test("native setup uses existing surface actions and does not claim an automatic key", () => {
  const setup = ownerBindingStatus({ _tag: "Ready", snapshot: unowned }, false)
  expect(setup).toMatchObject({ _tag: "Confirm", actions: [{ id: OWNER_BINDING_ACTION, label: "Set up owner", enabled: true }] })
  expect(JSON.stringify(setup)).not.toContain("nostrsigner:")
  expect(ownerBindingStatus({ _tag: "Ready", snapshot: owned }, false)).toBeUndefined()
  expect(ownerBindingStatus({ _tag: "Ready", snapshot: owned }, true)).toMatchObject({ _tag: "Confirm", actions: [{ id: OWNER_BINDING_CONTINUE, label: "Open library" }] })
  expect(ownerBindingStatus({ _tag: "Unsupported" }, true)).toBeUndefined()
})

test("missing signer, denial, and observation failure remain visible and retryable", () => {
  const missing = { ...unowned, signerRequirement: "Install a compatible NIP-55 signer such as Amber" }
  expect(ownerBindingStatus({ _tag: "Ready", snapshot: missing }, true)).toMatchObject({ _tag: "Confirm", actions: [{ label: "Check for signer" }] })
  const denied = { ...unowned, personSigner: { _tag: OwnerSignerKind.Denied, message: "The signer denied the request" } }
  expect(ownerBindingStatus({ _tag: "Ready", snapshot: denied }, true)).toMatchObject({ _tag: "Confirm", detail: expect.stringContaining("denied") })
  expect(ownerBindingStatus({ _tag: "Submitting", snapshot: unowned }, true)).toMatchObject({ _tag: "Busy" })
  expect(ownerBindingStatus({ _tag: "Problem", message: "Cannot read status" }, true)).toMatchObject({ _tag: "Confirm", kicker: "Could not read identity", actions: [{ label: "Check identity" }] })
})

test("HTTP setup excludes duplicate commands, observes ownership, and reloads changed peer membership only", async () => {
  let current = structuredClone(unowned)
  let starts = 0
  let changed = 0
  let finished = 0
  let updatedAt = 1
  let peerState = "ready"
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    if (request.headers.get("authorization") !== "Bearer fixture-capability") return new Response(null, { status: 401 })
    const body = await request.json() as { _tag: string }
    if (body._tag === "system.identity.owner-binding.start") {
      ++starts
      current = { ...unowned, personSigner: { _tag: OwnerSignerKind.Pending, message: "Waiting" } }
      await Bun.sleep(20)
    }
    const payload = body._tag === "app.peer.list"
      ? { peers: [{ devicePublicKey: "44".repeat(32), label: "Linux peer", state: peerState, updatedAt }] } : current
    return Response.json({ _tag: body._tag, outcome: { _tag: "Ok", payload } })
  } })
  const controller = createOwnerBindingController(createHttpKorridClient(`http://127.0.0.1:${server.port}`, "fixture-capability"), {
    changed: () => { ++changed }, finish: () => { ++finished },
  })
  const stop = controller.start()
  try {
    await waitFor(() => controller.snapshot()._tag === "Ready")
    controller.act(OWNER_BINDING_ACTION)
    controller.act(OWNER_BINDING_ACTION)
    expect(controller.snapshot()._tag).toBe("Submitting")
    await waitFor(() => controller.snapshot()._tag === "Ready")
    expect(starts).toBe(1)
    current = structuredClone(owned)
    await controller.refresh()
    expect(changed).toBe(1)
    updatedAt = 2
    await controller.refresh()
    expect(changed).toBe(1)
    peerState = "failed"
    await controller.refresh()
    expect(changed).toBe(2)
    controller.act(OWNER_BINDING_CONTINUE)
    expect(finished).toBe(1)
    controller.act(OWNER_BINDING_ACTION)
    expect(starts).toBe(1)
  } finally { stop(); server.stop(true) }
})

test("browser preview does not expose a native signing action", async () => {
  const controller = createOwnerBindingController(createInMemoryKorridClient(), { changed() {}, finish() {} })
  const stop = controller.start()
  try {
    await waitFor(() => controller.snapshot()._tag === "Unsupported")
    expect(ownerBindingStatus(controller.snapshot(), true)).toBeUndefined()
  } finally { stop() }
})
