import { expect, test } from "bun:test"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"
import { createInMemoryStoreAcquisition, SAMPLE_STORE } from "./store-acquisition-client"
import type { StoreAcquisitionRpcRequest } from "./store-acquisition-types"

test("the six acquisition methods use the tagged /rpc wire with the capability", async () => {
  const implementation = createInMemoryStoreAcquisition(SAMPLE_STORE)
  const requests: StoreAcquisitionRpcRequest[] = []
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    expect(request.headers.get("authorization")).toBe("Bearer private-capability")
    const rpc = await request.json() as StoreAcquisitionRpcRequest
    requests.push(rpc)
    const outcome = await (() => {
      switch (rpc._tag) {
        case "app.acquisition.providers": return implementation.acquisitionProviders()
        case "app.acquisition.search": return implementation.acquisitionSearch(rpc.payload)
        case "app.acquisition.details": return implementation.acquisitionDetails(rpc.payload)
        case "app.acquisition.acquire": return implementation.acquisitionAcquire(rpc.payload)
        case "app.acquisition.acquire-status": return implementation.acquisitionAcquireStatus(rpc.payload.jobId)
        case "app.acquisition.acquire-cancel": return implementation.acquisitionAcquireCancel(rpc.payload.jobId)
      }
    })()
    return Response.json({ _tag: rpc._tag, outcome })
  } })
  try {
    const client = createHttpKorridClient(server.url.origin, "private-capability")
    expect(await client.acquisitionProviders()).toMatchObject({ _tag: "Ok", payload: { providers: [{ id: "@korri:homebrewhub" }, {}, {}] } })
    expect(await client.acquisitionSearch({ query: "2048", providerIds: ["@korri:homebrewhub"] }))
      .toMatchObject({ _tag: "Ok", payload: { claims: [{ id: "2048gb", title: "2048" }, { id: "basil-termini" }] } })
    expect(await client.acquisitionDetails({ providerId: "@korri:homebrewhub", id: "2048gb" }))
      .toMatchObject({ _tag: "Ok", payload: { artifact: { system: "gb", file: { sizeBytes: 32_768 } } } })
    const job = await client.acquisitionAcquire({ providerId: "@korri:homebrewhub", id: "2048gb" })
    expect(job).toMatchObject({ _tag: "Ok", payload: { jobId: "job-1", state: { _tag: "Resolving" } } })
    expect(await client.acquisitionAcquireStatus("job-1")).toMatchObject({ _tag: "Ok", payload: { jobId: "job-1" } })
    expect(await client.acquisitionAcquireCancel("job-1")).toMatchObject({ _tag: "Ok", payload: { state: { _tag: "Cancelled" } } })
    expect(await client.acquisitionAcquireStatus("job-9")).toMatchObject({ _tag: "Err", payload: { code: "NotFound" } })
    expect(requests.map(request => request._tag)).toEqual([
      "app.acquisition.providers",
      "app.acquisition.search",
      "app.acquisition.details",
      "app.acquisition.acquire",
      "app.acquisition.acquire-status",
      "app.acquisition.acquire-cancel",
      "app.acquisition.acquire-status",
    ])
    expect(requests[1]).toEqual({ _tag: "app.acquisition.search", payload: { query: "2048", providerIds: ["@korri:homebrewhub"] } })
    expect(requests[3]).toEqual({ _tag: "app.acquisition.acquire", payload: { providerId: "@korri:homebrewhub", id: "2048gb" } })
    expect(requests[4]).toEqual({ _tag: "app.acquisition.acquire-status", payload: { jobId: "job-1" } })
  } finally {
    server.stop(true)
  }
})

test("an unreachable korrid is an Err the store can show, never a throw", async () => {
  const client = createHttpKorridClient("http://127.0.0.1:9", "private-capability")
  expect(await client.acquisitionProviders()).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
  expect(await client.acquisitionAcquireStatus("job-1")).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
})

test("a game the in-memory brain imports joins its catalog", async () => {
  const client = createInMemoryKorridClient({ games: [] })
  const job = await client.acquisitionAcquire({ providerId: "@korri:homebrewhub", id: "2048gb" })
  if (job._tag !== "Ok") throw new Error("expected a job")
  let state = job.payload.state
  for (let i = 0; i < 20 && state._tag !== "Imported"; i++) {
    const status = await client.acquisitionAcquireStatus(job.payload.jobId)
    if (status._tag === "Ok") state = status.payload.state
  }
  expect(state).toEqual({ _tag: "Imported", gameId: "store-korrihomebrewhub-2048gb" })
  expect(await client.catalogSnapshot()).toMatchObject({ _tag: "Ok", payload: { games: [{ id: "store-korrihomebrewhub-2048gb", title: "2048" }] } })
})
