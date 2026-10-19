import { expect, test } from "bun:test"
import type { AcquisitionRpcRequest } from "./acquisition-types"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

const steam = { id: "hk", title: "Hollow Knight", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const artifacts = [
  { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000 },
  { id: "@korri:proton/cachyos", title: "Proton-CachyOS", bytesTotal: 340_000_000, ready: true },
]

test("acquisition status/start/cancel use the tagged /rpc wire with the capability, and resume keeps the bytes", async () => {
  const implementation = createInMemoryKorridClient({
    games: [steam], artifacts, gameArtifacts: { hk: artifacts.map(artifact => artifact.id) },
    acquisitionTickMs: 5, acquisitionBytesPerTick: 100_000_000, acquisitionUnpackTicks: 1,
  })
  const requests: AcquisitionRpcRequest[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    expect(request.headers.get("authorization")).toBe("Bearer private-capability")
    const rpc = await request.json() as AcquisitionRpcRequest
    requests.push(rpc)
    switch (rpc._tag) {
      case "app.acquisition.status": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionStatus(rpc.payload.gameId) })
      case "app.acquisition.start": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionStart(rpc.payload.artifactIds) })
      case "app.acquisition.cancel": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionCancel(rpc.payload.artifactId) })
      default: return new Response("Unexpected request", { status: 400 })
    }
  } })
  try {
    const client = createHttpKorridClient(server.url.origin, "private-capability")
    expect(await client.acquisitionStatus()).toMatchObject({ _tag: "Ok", payload: { artifacts: [
      { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000, state: { _tag: "Missing" } },
      { id: "@korri:proton/cachyos", state: { _tag: "Ready" } },
    ] } })
    expect(await client.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok" })
    expect(await client.acquisitionStatus("nope")).toMatchObject({ _tag: "Err", payload: { code: "GameNotFound" } })
    const started = await client.acquisitionStart(["@korri:fex/rootfs"])
    expect(started).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: "@korri:fex/rootfs", state: { _tag: "Downloading", bytesDone: 0, bytesTotal: 1_160_000_000 } }] } })
    await new Promise(resolve => setTimeout(resolve, 12))
    const cancelled = await client.acquisitionCancel("@korri:fex/rootfs")
    expect(cancelled).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Missing" } }] } })
    const resumed = await client.acquisitionStart(["@korri:fex/rootfs"])
    const resumedState = resumed._tag === "Ok" ? resumed.payload.artifacts[0]?.state : undefined
    expect(resumedState?._tag === "Downloading" && resumedState.bytesDone).toBeGreaterThan(0)
    expect(requests.slice(0, 3)).toEqual([
      { _tag: "app.acquisition.status", payload: {} },
      { _tag: "app.acquisition.status", payload: { gameId: "hk" } },
      { _tag: "app.acquisition.status", payload: { gameId: "nope" } },
    ])
    expect(requests.filter(rpc => rpc._tag === "app.acquisition.start")).toEqual([
      { _tag: "app.acquisition.start", payload: { artifactIds: ["@korri:fex/rootfs"] } },
      { _tag: "app.acquisition.start", payload: { artifactIds: ["@korri:fex/rootfs"] } },
    ])
    expect(requests.filter(rpc => rpc._tag === "app.acquisition.cancel")).toEqual([
      { _tag: "app.acquisition.cancel", payload: { artifactId: "@korri:fex/rootfs" } },
    ])
    // Let the resumed download finish so the test leaves no timer behind.
    await new Promise(resolve => setTimeout(resolve, 120))
    expect(await client.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Ready" } }, { state: { _tag: "Ready" } }] } })
  } finally { await server.stop(true) }
})

test("acquisition methods report real HTTP permission rejection without another operation", async () => {
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(null, { status: 403 }) })
  try {
    const client = createHttpKorridClient(server.url.origin, "read-only")
    expect(await client.acquisitionStatus()).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
    expect(await client.acquisitionStart(["x"])).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
    expect(await client.acquisitionCancel("x")).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
  } finally { await server.stop(true) }
})

test("in-memory: a download that fails reports korrid-style plain copy and resumes on the next start", async () => {
  const korrid = createInMemoryKorridClient({
    games: [steam], gameArtifacts: { hk: ["@korri:fex/rootfs"] },
    artifacts: [{ id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 100, failure: { message: "The connection stalled.", atFraction: 0.5 } }],
    acquisitionTickMs: 2, acquisitionBytesPerTick: 30,
  })
  await korrid.acquisitionStart(["@korri:fex/rootfs"])
  await new Promise(resolve => setTimeout(resolve, 30))
  expect(await korrid.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [
    { state: { _tag: "Failed", reason: { message: "The connection stalled." } } },
  ] } })
  const retried = await korrid.acquisitionStart(["@korri:fex/rootfs"])
  expect(retried).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Downloading", bytesDone: 60 } }] } })
  await korrid.acquisitionCancel("@korri:fex/rootfs")
})
