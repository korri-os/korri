import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import { PeerListState, type CatalogSnapshotOutcome, type Game, type PeerListOutcome } from "@contracts/generated/korrid"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { surfaceModelFrom } from "./surface-model"
import { useLaunchables, type Launchables } from "./use-launchables"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
})
const waitFor = async (predicate: () => boolean) => {
  for (let attempt = 0; attempt < 250; attempt += 1) {
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 10)) })
    if (predicate()) return
  }
  throw new Error("condition did not become true")
}
const deferred = <A,>() => {
  let resolve!: (value: A) => void
  const promise = new Promise<A>(complete => { resolve = complete })
  return { promise, resolve }
}
const game: Game = {
  id: "wl4", title: "Wario Land 4", host: "Peer",
  supportsRunnerSelection: false, source: { label: "Peer", isLocal: false },
}
const peers = (state: PeerListState, updatedAt = 1): PeerListOutcome => ({
  _tag: "Ok", payload: { peers: [{ devicePublicKey: "aa".repeat(32), label: "Peer", state, updatedAt }] },
})

async function mount(client: KorridClient) {
  let value!: Launchables
  function Probe() { value = useLaunchables(client); return null }
  const root = createRoot(document.createElement("div"))
  roots.push(root)
  await act(async () => root.render(<Probe />))
  return { current: () => value, root }
}

function fixture(initial = peers(PeerListState.Loading)) {
  let peerList = initial
  let games: Game[] = []
  let reads = 0
  let peerReads = 0
  const client: KorridClient = {
    ...createInMemoryKorridClient({ games: [] }),
    async peerList() { peerReads += 1; return structuredClone(peerList) },
    async catalogSnapshot() { reads += 1; return { _tag: "Ok", payload: { games: [...games] } } },
  }
  return {
    client, reads: () => reads, peerReads: () => peerReads,
    setPeers(next: PeerListOutcome) { peerList = next },
    setGames(next: Game[]) { games = next },
  }
}

test("an empty cold-start catalog stays reading until peers connect, then shows their games without remount", async () => {
  const config = fixture()
  const harness = await mount(config.client)
  await waitFor(() => config.reads() > 0)
  expect(surfaceModelFrom(harness.current().state).catalog._tag).toBe("Loading")
  config.setGames([game])
  config.setPeers(peers(PeerListState.Ready))
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({
    _tag: "Ready", games: [expect.objectContaining({ title: game.title })],
  })
  expect(config.reads()).toBeGreaterThan(1)
})

test("a genuinely empty library settles when the loading peers become ready", async () => {
  const config = fixture()
  const harness = await mount(config.client)
  await waitFor(() => config.reads() > 0)
  expect(harness.current().state._tag).toBe("Loading")
  config.setPeers(peers(PeerListState.Ready))
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(surfaceModelFrom(harness.current().state).catalog._tag).toBe("Empty")
})

test("a peer discovered after the initial empty roster refreshes the library", async () => {
  const config = fixture({ _tag: "Ok", payload: { peers: [] } })
  const harness = await mount(config.client)
  await waitFor(() => harness.current().state._tag === "Ready")
  config.setGames([game])
  config.setPeers(peers(PeerListState.Ready))
  await waitFor(() => config.reads() > 1)
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({
    _tag: "Ready", games: [expect.objectContaining({ title: game.title })],
  })
})

test("available games remain usable while another peer loads", async () => {
  const config = fixture()
  config.setGames([game])
  const harness = await mount(config.client)
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({ _tag: "Ready", games: [expect.objectContaining({ title: game.title })] })
})

test("peer observation failures do not become a silent nothing-to-play state", async () => {
  const config = fixture({ _tag: "Err", payload: { code: "PeerListUnavailable", message: "peer directory unavailable" } })
  const harness = await mount(config.client)
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(harness.current().state).toMatchObject({ notice: { _tag: "Catalog", message: expect.stringContaining("PeerListUnavailable") } })
  config.setGames([game])
  config.setPeers(peers(PeerListState.Ready))
  await waitFor(() => config.reads() > 1)
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({ _tag: "Ready", games: [expect.objectContaining({ title: game.title })] })
  expect(harness.current().state).toMatchObject({ notice: null })
})

test("an empty read started before Ready cannot finalize the library", async () => {
  const config = fixture()
  const initial = deferred<CatalogSnapshotOutcome>()
  let reads = 0
  const client: KorridClient = { ...config.client, catalogSnapshot() {
    return ++reads === 1 ? initial.promise : config.client.catalogSnapshot()
  } }
  const harness = await mount(client)
  config.setGames([game])
  config.setPeers(peers(PeerListState.Ready))
  await act(async () => initial.resolve({ _tag: "Ok", payload: { games: [] } }))
  expect(surfaceModelFrom(harness.current().state).catalog._tag).toBe("Loading")
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({ _tag: "Ready", games: [expect.objectContaining({ title: game.title })] })
})

test("observation timestamps do not trigger catalog refresh loops", async () => {
  const config = fixture(peers(PeerListState.Ready))
  config.setGames([game])
  const harness = await mount(config.client)
  await waitFor(() => harness.current().state._tag === "Ready")
  config.setPeers(peers(PeerListState.Ready, 2))
  await waitFor(() => config.peerReads() > 1)
  expect(config.reads()).toBe(1)
  expect(harness.current().state._tag).toBe("Ready")
})

test("peer failure ends the wait with its catalog error and recovery refreshes again", async () => {
  const config = fixture()
  let failCatalog = false
  const harness = await mount({ ...config.client, async catalogSnapshot() {
    return failCatalog ? {
      _tag: "Err", payload: { code: "PeerUnreachable", message: "Peer is offline" },
    } : config.client.catalogSnapshot()
  } })
  await waitFor(() => config.reads() > 0)
  failCatalog = true
  config.setPeers(peers(PeerListState.Failed))
  await waitFor(() => harness.current().state._tag === "Ready")
  expect(harness.current().state).toMatchObject({ notice: { _tag: "Catalog", message: expect.stringContaining("PeerUnreachable") } })
  failCatalog = false
  config.setGames([game])
  config.setPeers(peers(PeerListState.Ready))
  await waitFor(() => config.reads() > 1)
  expect(surfaceModelFrom(harness.current().state).catalog).toMatchObject({ _tag: "Ready", games: [expect.objectContaining({ title: game.title })] })
  expect(harness.current().state).toMatchObject({ notice: null })
})

test("unmount retires single-flight peer polling and late peer responses", async () => {
  const config = fixture()
  const pending = deferred<PeerListOutcome>()
  let peerReads = 0
  const client: KorridClient = { ...config.client, peerList() {
    peerReads += 1
    return peerReads === 1 ? config.client.peerList() : pending.promise
  } }
  const harness = await mount(client)
  await waitFor(() => peerReads > 1)
  await act(async () => { await new Promise(complete => setTimeout(complete, 1100)) })
  expect(peerReads).toBe(2)
  await act(async () => harness.root.unmount())
  roots.splice(roots.indexOf(harness.root), 1)
  const reads = config.reads()
  await act(async () => pending.resolve(peers(PeerListState.Ready)))
  await act(async () => { await new Promise(complete => setTimeout(complete, 1100)) })
  expect(config.reads()).toBe(reads)
  expect(peerReads).toBe(2)
})
