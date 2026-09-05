import { describe, expect, it } from "bun:test"
import { createServer } from "node:http"
import { PeerListState, type PeerListOutcome } from "@contracts/generated/korrid"
import { createInMemoryKorridClient } from "./client"

const peers = [
  {
    devicePublicKey: "aa".repeat(32),
    label: "Loading peer",
    state: PeerListState.Loading,
    updatedAt: 1_700_000_000,
  },
  {
    devicePublicKey: "bb".repeat(32),
    label: "Ready peer",
    state: PeerListState.Ready,
    updatedAt: 1_700_000_001,
  },
  {
    devicePublicKey: "cc".repeat(32),
    label: "Failed peer",
    state: PeerListState.Failed,
    updatedAt: 1_700_000_002,
    lastError: "Authenticated peer operation failed",
  },
]
const ready: PeerListOutcome = { _tag: "Ok", payload: { peers } }

async function serveBrainResponse(tag: string, status: number, outcome: PeerListOutcome) {
  const requests: { path: string | undefined; authorization: string | undefined; body: unknown }[] = []
  const server = createServer(async (request, response) => {
    const chunks: Buffer[] = []
    for await (const chunk of request) chunks.push(Buffer.from(chunk))
    requests.push({
      path: request.url,
      authorization: request.headers.authorization,
      body: JSON.parse(Buffer.concat(chunks).toString()),
    })
    response.writeHead(status, { "content-type": "application/json" })
    response.end(JSON.stringify({ _tag: tag, outcome }))
  })
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
  const address = server.address()
  if (!address || typeof address === "string") throw new Error("missing HTTP address")
  return {
    origin: `http://127.0.0.1:${address.port}`,
    requests,
    stop: () => new Promise<void>((resolve, reject) => {
      server.close((error) => error ? reject(error) : resolve())
      server.closeAllConnections()
    }),
  }
}

async function readHttpPeerList(origin: string, capability: string): Promise<PeerListOutcome> {
  // Run the actual client with native fetch. Happy DOM's fetch removes manually
  // supplied Authorization headers on cross-origin requests, unlike browsers.
  const child = Bun.spawn([process.execPath, "--eval", `
    import { createHttpKorridClient } from ${JSON.stringify(new URL("./client.ts", import.meta.url).pathname)};
    const result = await createHttpKorridClient(${JSON.stringify(origin)}, ${JSON.stringify(capability)}).peerList();
    process.stdout.write(JSON.stringify(result));
  `], { stdout: "pipe", stderr: "pipe" })
  const [output, errors, code] = await Promise.all([
    Bun.readableStreamToText(child.stdout),
    Bun.readableStreamToText(child.stderr),
    child.exited,
  ])
  expect({ code, errors }).toEqual({ code: 0, errors: "" })
  return JSON.parse(output)
}

describe("peerList", () => {
  it("sends an empty request with the local capability and preserves the typed snapshot", async () => {
    const server = await serveBrainResponse("app.peer.list", 200, ready)
    try {
      expect(await readHttpPeerList(server.origin, "peer-list-capability")).toEqual(ready)
      expect(server.requests).toEqual([{
        path: "/rpc",
        authorization: "Bearer peer-list-capability",
        body: { _tag: "app.peer.list", payload: {} },
      }])
    } finally {
      await server.stop()
    }
  })

  it("preserves RPC failures and rejects a wrong response tag or denied route", async () => {
    const failure: PeerListOutcome = {
      _tag: "Err",
      payload: { code: "PeerListUnavailable", message: "peer directory unavailable" },
    }
    for (const [tag, status, expected] of [
      ["app.peer.list", 200, failure],
      ["system.health", 200, undefined],
      ["app.peer.list", 403, undefined],
    ] as const) {
      const server = await serveBrainResponse(tag, status, failure)
      try {
        const outcome = await readHttpPeerList(server.origin, "capability")
        if (expected) expect(outcome).toEqual(expected)
        else expect(outcome).toMatchObject({ _tag: "Err", payload: { code: "BrainUnreachable" } })
      } finally {
        await server.stop()
      }
    }
  })

  it("has an empty sandbox and isolated deterministic configured snapshots", async () => {
    const seed = structuredClone(ready)
    const configured = createInMemoryKorridClient({ peerList: seed })
    const sandbox = createInMemoryKorridClient()
    expect(await sandbox.peerList()).toEqual({ _tag: "Ok", payload: { peers: [] } })
    expect(await configured.peerList()).toEqual(ready)
    // Neither the input object nor a returned snapshot owns client state.
    if (seed._tag === "Ok") seed.payload.peers[0]!.label = "mutated input"
    const first = await configured.peerList()
    if (first._tag === "Ok") first.payload.peers[0]!.label = "mutated output"
    expect(await configured.peerList()).toEqual(ready)
    expect(await sandbox.peerList()).toEqual({ _tag: "Ok", payload: { peers: [] } })
    const failed: PeerListOutcome = {
      _tag: "Err",
      payload: { code: "PeerListUnavailable", message: "peer directory unavailable" },
    }
    expect(await createInMemoryKorridClient({ peerList: failed }).peerList()).toEqual(failed)
    expect(peers.every((peer) => Number.isSafeInteger(peer.updatedAt))).toBe(true)
  })
})
