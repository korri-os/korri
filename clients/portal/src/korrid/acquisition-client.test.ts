import { expect, test } from "bun:test"
import type { RpcRequest } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

type AcquisitionRpcRequest = Extract<RpcRequest, { _tag: `app.acquisition.${string}` }>

const steam = { id: "hk", title: "Hollow Knight", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const artifacts = [
  { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000 },
  { id: "@korri:proton/cachyos", title: "Proton-CachyOS", bytesTotal: 340_000_000, ready: true },
]

test("acquisition status/start/cancel use the tagged /rpc wire with the capability, and resume keeps the bytes", async () => {
  const implementation = createInMemoryKorridClient({
    games: [steam], artifacts, gameArtifacts: { hk: artifacts.map(artifact => artifact.id) },
    acquisitionBytesPerStep: 100_000_000, acquisitionUnpackSteps: 1,
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
    // Each observation of the game advances the simulated download a step.
    expect(await client.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Downloading", bytesDone: 100_000_000 } }, { state: { _tag: "Ready" } }] } })
    const cancelled = await client.acquisitionCancel("@korri:fex/rootfs")
    expect(cancelled).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: "@korri:fex/rootfs", state: { _tag: "Missing" } }] } })
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
    // Observe it through: twelve steps of 100 MB, one Unpacking, then Ready.
    let last = await client.acquisitionStatus("hk")
    for (let i = 0; i < 16 && !(last._tag === "Ok" && last.payload.artifacts.every(artifact => artifact.state._tag === "Ready")); i++) {
      last = await client.acquisitionStatus("hk")
    }
    expect(last).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Ready" } }, { state: { _tag: "Ready" } }] } })
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
    acquisitionBytesPerStep: 30,
  })
  await korrid.acquisitionStart(["@korri:fex/rootfs"])
  await korrid.acquisitionStatus("hk") // 30
  expect(await korrid.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [
    { state: { _tag: "Failed", reason: { message: "The connection stalled." } } },
  ] } })
  const retried = await korrid.acquisitionStart(["@korri:fex/rootfs"])
  expect(retried).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Downloading", bytesDone: 60 } }] } })
})
