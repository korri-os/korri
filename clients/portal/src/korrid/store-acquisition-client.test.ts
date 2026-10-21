import { expect, test } from "bun:test"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"
import { createInMemoryStoreAcquisition, SAMPLE_STORE } from "./store-acquisition-client"
import type { StoreAcquisitionRpcRequest } from "./store-acquisition-types"

test("the six acquisition methods use the tagged /rpc wire with the capability", async () => {
  const implementation = createInMemoryStoreAcquisition({ ...SAMPLE_STORE, failingSearches: {} })
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
    expect(await client.acquisitionProviders()).toMatchObject({ _tag: "Ok", payload: { providers: [{ id: "@korri:pico8", title: "PICO-8 BBS" }] } })
    expect(await client.acquisitionSearch({ query: "celeste", providerIds: ["@korri:pico8"] }))
      .toMatchObject({ _tag: "Ok", payload: { claims: [{ _tag: "ProviderClaim", id: "celeste_classic_2-5", title: "Celeste Classic 2" }] } })
    expect(await client.acquisitionDetails({ providerId: "@korri:pico8", id: "celeste_classic_2-5" }))
      .toMatchObject({ _tag: "Ok", payload: { _tag: "ProviderClaimDetails", artifact: { system: "pico8", file: { name: "celeste_classic_2-5.p8.png" } } } })
    const job = await client.acquisitionAcquire({ providerId: "@korri:pico8", id: "celeste_classic_2-5" })
    expect(job).toMatchObject({ _tag: "Ok", payload: { jobId: "job-1", state: { _tag: "Resolving" } } })
    expect(await client.acquisitionAcquireStatus("job-1")).toMatchObject({ _tag: "Ok", payload: { jobId: "job-1" } })
    expect(await client.acquisitionAcquireCancel("job-1")).toMatchObject({ _tag: "Ok", payload: { state: { _tag: "Cancelled" } } })
    expect(await client.acquisitionAcquireStatus("job-9")).toMatchObject({ _tag: "Err", payload: { code: "AcquisitionUnknownJob" } })
    expect(requests.map(request => request._tag)).toEqual([
      "app.acquisition.providers",
      "app.acquisition.search",
      "app.acquisition.details",
      "app.acquisition.acquire",
      "app.acquisition.acquire-status",
      "app.acquisition.acquire-cancel",
      "app.acquisition.acquire-status",
    ])
    expect(requests[1]).toEqual({ _tag: "app.acquisition.search", payload: { query: "celeste", providerIds: ["@korri:pico8"] } })
    expect(requests[3]).toEqual({ _tag: "app.acquisition.acquire", payload: { providerId: "@korri:pico8", id: "celeste_classic_2-5" } })
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
  // acquire needs a claim korrid holds: details returns it.
  await client.acquisitionDetails({ providerId: "@korri:pico8", id: "celeste_classic_2-5" })
  const job = await client.acquisitionAcquire({ providerId: "@korri:pico8", id: "celeste_classic_2-5" })
  if (job._tag !== "Ok") throw new Error("expected a job")
  let state = job.payload.state
  for (let i = 0; i < 20 && state._tag !== "Imported"; i++) {
    const status = await client.acquisitionAcquireStatus(job.payload.jobId)
    if (status._tag === "Ok") state = status.payload.state
  }
  expect(state).toEqual({ _tag: "Imported", gameId: "store-korripico8-celeste_classic_2-5" })
  expect(await client.catalogSnapshot()).toMatchObject({ _tag: "Ok", payload: { games: [{ id: "store-korripico8-celeste_classic_2-5", title: "Celeste Classic 2" }] } })
})
