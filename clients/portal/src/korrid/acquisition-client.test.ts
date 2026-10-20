import { expect, test } from "bun:test"
import type { RpcRequest } from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

type AcquisitionRpcRequest = Extract<RpcRequest, { _tag: `app.acquisition.${string}` }>

const steam = { id: "hk", title: "Hollow Knight", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
const artifacts = [
  { id: "@korri:fex/rootfs", title: "FEX rootfs", bytesTotal: 1_160_000_000 },
  { id: "@korri:proton/cachyos", title: "Proton-CachyOS", bytesTotal: 340_000_000, ready: true },
]

test("acquisition status/start/cancel use the tagged /rpc wire with the capability, and Cancel deletes the bytes", async () => {
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
      case "app.acquisition.download": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionDownload(rpc.payload.gameId) })
      case "app.acquisition.remove": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionRemove(rpc.payload.gameId) })
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
    // korrid deleted the partial file (ADR 0013): the next start is from zero.
    const restarted = await client.acquisitionStart(["@korri:fex/rootfs"])
    const restartedState = restarted._tag === "Ok" ? restarted.payload.artifacts[0]?.state : undefined
    expect(restartedState).toMatchObject({ _tag: "Downloading", bytesDone: 0 })
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
    expect(await client.acquisitionDownload("x")).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
    expect(await client.acquisitionRemove("x")).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
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

test("download and remove use the tagged /rpc wire; the game's own artifacts carry release", async () => {
  const implementation = createInMemoryKorridClient({
    games: [steam], artifacts: [...artifacts, { id: "@game:hk/content", title: "Hollow Knight", bytesTotal: 50 }],
    gameArtifacts: { hk: artifacts.map(artifact => artifact.id) }, releaseArtifacts: { hk: ["@game:hk/content"] },
    acquisitionBytesPerStep: 2_000_000_000, acquisitionUnpackSteps: 0,
  })
  const requests: AcquisitionRpcRequest[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    const rpc = await request.json() as AcquisitionRpcRequest
    requests.push(rpc)
    switch (rpc._tag) {
      case "app.acquisition.status": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionStatus(rpc.payload.gameId) })
      case "app.acquisition.download": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionDownload(rpc.payload.gameId) })
      case "app.acquisition.remove": return Response.json({ _tag: rpc._tag, outcome: await implementation.acquisitionRemove(rpc.payload.gameId) })
      default: return new Response("Unexpected request", { status: 400 })
    }
  } })
  try {
    const client = createHttpKorridClient(server.url.origin, "private-capability")
    // The release's own first, then the runner's; one download at a time.
    expect(await client.acquisitionDownload("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [
      { id: "@game:hk/content", release: true, state: { _tag: "Downloading" } },
      { id: "@korri:fex/rootfs", state: { _tag: "Queued" } },
      { id: "@korri:proton/cachyos", state: { _tag: "Ready" } },
    ] } })
    // While anything of it downloads, korrid refuses to remove it.
    expect(await client.acquisitionRemove("hk")).toMatchObject({ _tag: "Err" })
    let last = await client.acquisitionStatus("hk")
    for (let i = 0; i < 8 && !(last._tag === "Ok" && last.payload.artifacts.every(artifact => artifact.state._tag === "Ready")); i++) {
      last = await client.acquisitionStatus("hk")
    }
    expect(last).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ state: { _tag: "Ready" } }, { state: { _tag: "Ready" } }, { state: { _tag: "Ready" } }] } })
    // Remove takes only the release's own; the runtime stays.
    expect(await client.acquisitionRemove("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [
      { id: "@game:hk/content", release: true, state: { _tag: "Missing" } },
      { id: "@korri:fex/rootfs", state: { _tag: "Ready" } },
      { id: "@korri:proton/cachyos", state: { _tag: "Ready" } },
    ] } })
    expect(requests.filter(rpc => rpc._tag !== "app.acquisition.status")).toEqual([
      { _tag: "app.acquisition.download", payload: { gameId: "hk" } },
      { _tag: "app.acquisition.remove", payload: { gameId: "hk" } },
      { _tag: "app.acquisition.remove", payload: { gameId: "hk" } },
    ])
  } finally { await server.stop(true) }
})

test("in-memory: a launch's start goes before the queue and pauses the download under way", async () => {
  const korrid = createInMemoryKorridClient({
    games: [steam, { ...steam, id: "celeste", title: "Celeste" }],
    artifacts: [{ id: "a", title: "A", bytesTotal: 100 }, { id: "b", title: "B", bytesTotal: 100 }],
    gameArtifacts: { hk: ["a"], celeste: ["b"] }, acquisitionBytesPerStep: 10,
  })
  await korrid.acquisitionDownload("hk")
  await korrid.acquisitionStatus() // a: 10
  expect(await korrid.acquisitionStart(["b"])).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: "b", state: { _tag: "Downloading", bytesDone: 0 } }] } })
  expect(await korrid.acquisitionStatus("hk")).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: "a", state: { _tag: "Queued" } }] } })
  expect(await korrid.acquisitionRemove("hk")).toMatchObject({ _tag: "Err", payload: { code: "AcquisitionBusy" } })
  expect(await korrid.acquisitionCancel("a")).toMatchObject({ _tag: "Ok", payload: { artifacts: [{ id: "a", state: { _tag: "Missing" } }] } })
})
