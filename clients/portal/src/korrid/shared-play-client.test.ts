import { expect, test } from "bun:test"
import { SessionControlFailureReason, type ActiveSession, type Invite } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

const invite: Invite = { inviteId: "opaque", devicePublicKey: "far", deviceLabel: "Pocket Mini", personName: "Simon",
  session: { launchId: "exact", gameId: "game", title: "Game", phase: "running" } }

test("the HTTP client speaks all three local-only invite tags to its own korrid", async () => {
  const requests: unknown[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    const body = await request.json() as { _tag: string; payload: object }
    requests.push(body)
    const payload = body._tag === "app.invites.list" ? { invites: [invite] }
      : body._tag === "app.invites.streamPlay" ? invite.session : {}
    return Response.json({ _tag: body._tag, outcome: { _tag: "Ok", payload } })
  } })
  try {
    const client = createHttpKorridClient(`http://127.0.0.1:${server.port}`, "test")
    expect(await client.invitesList()).toMatchObject({ _tag: "Ok", payload: { invites: [invite] } })
    expect(await client.invitesStreamPlay("opaque")).toMatchObject({ _tag: "Ok", payload: { launchId: "exact" } })
    expect(await client.invitesDismiss("opaque")).toEqual({ _tag: "Ok", payload: {} })
    expect(requests).toEqual([
      { _tag: "app.invites.list", payload: {} }, { _tag: "app.invites.streamPlay", payload: { inviteId: "opaque" } },
      { _tag: "app.invites.dismiss", payload: { inviteId: "opaque" } },
    ])
  } finally { await server.stop(true) }
})

test("the dev client retains the holder on Leave and denies session controls to a stream player", async () => {
  let far: ActiveSession | undefined = { ...invite.session }
  const client = createInMemoryKorridClient({ invites: [invite], streamSession: () => far })
  expect((await client.invitesStreamPlay("opaque"))._tag).toBe("Ok")
  expect(await client.sessionControls("exact")).toMatchObject({ _tag: "Err", payload: { reason: SessionControlFailureReason.NotSessionStarter } })
  expect(await client.invokeSessionControl("exact", "plugin-control")).toMatchObject({ _tag: "Err", payload: { reason: SessionControlFailureReason.NotSessionStarter } })
  expect(await client.invitesList()).toEqual({ _tag: "Ok", payload: { invites: [] } })
  await client.sessionEnd("exact")
  expect(far?.phase).toBe("running")
  await client.invitesStreamPlay("opaque")
  far = undefined
  expect(await client.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { streamPlayEnded: { launchId: "exact", deviceLabel: "Pocket Mini", cause: "sessionEnded" } } })
})

test("person-name accepts an empty setup step, rejects over 64 characters, and respects signer editability", async () => {
  const client = createInMemoryKorridClient()
  const settings = await client.settingsSnapshot()
  if (settings._tag !== "Ok") throw Error("settings unavailable")
  expect(settings.payload.editableSettingIds).toContain("person-name")
  expect(await client.updateSetting(settings.payload.revision, "person-name", "x".repeat(65))).toMatchObject({ _tag: "Err", payload: { code: "SettingsInvalid" } })
  expect(await client.updateSetting(settings.payload.revision, "person-name", "")).toMatchObject({ _tag: "Ok", payload: { personName: "" } })
  const readonly = createInMemoryKorridClient({ settings: { ...settings.payload, editableSettingIds: [] } })
  expect(await readonly.updateSetting(settings.payload.revision, "person-name", "Simon")).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
})
