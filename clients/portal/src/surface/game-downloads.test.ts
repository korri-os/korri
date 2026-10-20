import { expect, test } from "bun:test"
import type { AcquisitionOutcome, ArtifactStatus } from "@contracts/generated/korrid"
import { createGameDownloads } from "./game-downloads"

const rootfs = (state: ArtifactStatus["state"]): ArtifactStatus => ({
  id: "@korri:fex/rootfs", title: "FEX rootfs", version: "archlinux-2026-01-08", bytesTotal: 1_160_000_000, state,
})
const proton = (state: ArtifactStatus["state"]): ArtifactStatus => ({
  id: "@korri:proton/cachyos", title: "Proton-CachyOS", version: "11.0", bytesTotal: 340_000_000, state,
})
const content = (state: ArtifactStatus["state"]): ArtifactStatus => ({
  id: "@megaman:rnr/zip", title: "Mega Man Rock N Roll", version: "1.0", bytesTotal: 500_000_000, state, release: true,
})
const ready = { _tag: "Ready", path: "/cache/x" } as const
const missing = { _tag: "Missing" } as const
const queued = { _tag: "Queued" } as const
const downloading = (bytesDone: number, bytesTotal?: number) =>
  ({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone, ...(bytesTotal === undefined ? {} : { bytesTotal }) }) as const
const ok = (artifacts: ArtifactStatus[]): AcquisitionOutcome => ({ _tag: "Ok", payload: { artifacts } })
const flush = () => new Promise(resolve => setTimeout(resolve, 0))
const settle = async (times = 4) => { for (let i = 0; i < times; i++) await flush() }
const mm = { id: "mm", surfaceId: "game:mm", title: "Mega Man Rock N Roll" }

/** A korrid whose status answers come from a function of the call, so a
 * test states the world rather than the order of reads. */
function brain(world: { all: () => ArtifactStatus[]; game: (id: string) => ArtifactStatus[] | undefined }) {
  const calls: { method: string; args: unknown }[] = []
  const answers = { download: [] as AcquisitionOutcome[], remove: [] as AcquisitionOutcome[], cancel: [] as AcquisitionOutcome[] }
  return {
    calls,
    answers,
    client: {
      acquisitionStatus: async (gameId?: string) => {
        calls.push({ method: "status", args: gameId })
        if (gameId === undefined) return ok(world.all())
        const artifacts = world.game(gameId)
        return artifacts === undefined
          ? { _tag: "Err" as const, payload: { code: "GameNotFound", message: "Game is not available" } }
          : ok(artifacts)
      },
      acquisitionDownload: async (gameId: string) => {
        calls.push({ method: "download", args: gameId })
        return answers.download.shift()!
      },
      acquisitionRemove: async (gameId: string) => {
        calls.push({ method: "remove", args: gameId })
        return answers.remove.shift()!
      },
      acquisitionCancel: async (artifactId: string) => {
        calls.push({ method: "cancel", args: artifactId })
        return answers.cancel.shift()!
      },
    },
  }
}

test("one device-wide read when all is Ready; per-game reads otherwise; sizes summed or unknown", async () => {
  let all = [rootfs(ready), proton(ready)]
  const games: Record<string, ArtifactStatus[]> = {}
  const korrid = brain({ all: () => all, game: id => games[id] })
  const downloads = createGameDownloads(korrid.client)
  await downloads.refresh(["steam:1", "steam:2", "gba:1"])
  expect(downloads.getFacts().size).toBe(0)
  expect(korrid.calls).toEqual([{ method: "status", args: undefined }])

  const wine: ArtifactStatus = { id: "@korri:wine/unsized", title: "Wine", version: "9", state: missing }
  all = [rootfs(missing), proton(ready), wine]
  games["steam:1"] = [rootfs(missing), proton(ready)]
  // An artifact is one thing on the device: its size is the same in every read.
  games["steam:2"] = [wine, proton(ready)]
  games["gba:1"] = []
  await downloads.refresh()
  expect(korrid.calls.slice(1).map(call => call.args)).toEqual([undefined, "steam:1", "steam:2", "gba:1"])
  const facts = downloads.getFacts()
  expect(facts.get("steam:1")).toMatchObject({ needs: true, bytes: 1_160_000_000, active: false, queued: false, ownReady: false })
  expect(facts.get("steam:2")).toMatchObject({ needs: true, active: false })
  expect(facts.get("steam:2")?.bytes).toBeUndefined()
  expect(facts.get("gba:1")).toMatchObject({ needs: false })
  downloads.dispose()
})

test("queued, then a forward-only bar over everything that was missing, then nothing to download", async () => {
  let world: ArtifactStatus[] = [content(queued), proton(queued)]
  const korrid = brain({ all: () => world, game: () => world })
  const downloads = createGameDownloads(korrid.client, { pollIntervalMs: 1 })
  await downloads.refresh(["mm"])
  expect(downloads.getFacts().get("mm")).toMatchObject({ needs: true, bytes: 840_000_000, active: true, stoppable: true, queued: true })
  expect(downloads.getFacts().get("mm")?.progress).toBeUndefined()

  // While it moves, the poll is one device-wide read a tick.
  const before = korrid.calls.length
  world = [content(downloading(100_000_000, 500_000_000)), proton(queued)]
  await settle(6)
  expect(korrid.calls.slice(before).every(call => call.method === "status" && call.args === undefined)).toBe(true)
  expect(downloads.getFacts().get("mm")).toMatchObject({ queued: false, progress: { done: 100_000_000, total: 840_000_000 } })

  // The game's files are in; the runtime's turn. The bar does not go back.
  world = [content(ready), proton(downloading(40_000_000, 340_000_000))]
  await settle(6)
  expect(downloads.getFacts().get("mm")).toMatchObject({ progress: { done: 540_000_000, total: 840_000_000 }, bytes: 340_000_000 })

  world = [content(ready), proton({ _tag: "Unpacking" })]
  await settle(6)
  expect(downloads.getFacts().get("mm")).toMatchObject({ stoppable: false, progress: { done: 0 } })
  expect(downloads.getFacts().get("mm")?.progress?.total).toBeUndefined()

  world = [content(ready), proton(ready)]
  await settle(6)
  expect(downloads.getFacts().get("mm")).toMatchObject({ needs: false, active: false, ownReady: true })
  // Nothing under way: the poll stops.
  const settledAt = korrid.calls.length
  await settle(8)
  expect(korrid.calls.length).toBe(settledAt)
  downloads.dispose()
})

test("a game whose files are all Ready is read once, when asked about, and then knows it has a download to remove", async () => {
  const korrid = brain({ all: () => [content(ready)], game: () => [content(ready)] })
  const downloads = createGameDownloads(korrid.client)
  await downloads.refresh(["mm"])
  expect(downloads.getFacts().has("mm")).toBe(false)
  downloads.learn("mm")
  downloads.learn("mm")
  await settle()
  expect(korrid.calls.filter(call => call.args === "mm")).toHaveLength(1)
  expect(downloads.getFacts().get("mm")).toMatchObject({ needs: false, ownReady: true })
  downloads.learn("mm")
  await settle()
  expect(korrid.calls.filter(call => call.args === "mm")).toHaveLength(1)
  downloads.dispose()
})

test("download, cancel and remove go to korrid once each, and a refusal is a plain Problem against the game", async () => {
  let world: ArtifactStatus[] = [content(missing), proton(ready)]
  const korrid = brain({ all: () => world, game: () => world })
  const downloads = createGameDownloads(korrid.client, { pollIntervalMs: 1 })
  await downloads.refresh(["mm"])

  korrid.answers.download.push(ok([content(queued), proton(ready)]))
  world = [content(queued), proton(ready)]
  downloads.download(mm)
  expect(downloads.isPending("mm")).toBe(true)
  downloads.download(mm)
  await settle()
  expect(korrid.calls.filter(call => call.method === "download")).toEqual([{ method: "download", args: "mm" }])
  expect(downloads.isPending("mm")).toBe(false)
  expect(downloads.getFacts().get("mm")).toMatchObject({ queued: true })

  world = [content(downloading(1, 500_000_000)), proton(ready)]
  await settle(4)
  korrid.answers.cancel.push(ok([content(missing)]))
  world = [content(missing), proton(ready)]
  downloads.cancel(mm)
  await settle()
  // Only what is queued or downloading; the Ready runtime is never touched.
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([{ method: "cancel", args: "@megaman:rnr/zip" }])
  expect(downloads.getFacts().get("mm")).toMatchObject({ needs: true, active: false })

  korrid.answers.remove.push({ _tag: "Err", payload: { code: "ActiveSessionConflict", message: "session active" } })
  downloads.remove(mm)
  await settle()
  expect(downloads.getStatus()).toEqual({
    _tag: "Problem", kicker: "Couldn't remove the download",
    reason: "Mega Man Rock N Roll is running. Stop it first, then remove the download.",
    canRetry: false, gameId: "game:mm", gameTitle: "Mega Man Rock N Roll",
  })
  expect(downloads.dismiss()).toBe(true)
  expect(downloads.getStatus()).toBeUndefined()
  expect(downloads.dismiss()).toBe(false)

  // korrid's refusal while anything of it is under way is already plain words.
  korrid.answers.remove.push({ _tag: "Err", payload: { code: "AcquisitionBusy", message: "This game is still downloading. Cancel the download first." } })
  downloads.remove(mm)
  await settle()
  expect(downloads.getStatus()).toMatchObject({ kicker: "Couldn't remove the download", reason: "This game is still downloading. Cancel the download first." })
  downloads.dismiss()

  korrid.answers.download.push({ _tag: "Err", payload: { code: "BrainUnreachable", message: "fetch failed" } })
  downloads.download(mm)
  await settle()
  expect(downloads.getStatus()).toMatchObject({ kicker: "Couldn't download Mega Man Rock N Roll", reason: "Korri did not answer. Try again in a moment." })
  downloads.dispose()
})

test("a failed background download keeps korrid's words for the next try", async () => {
  const world = [content({ _tag: "Failed", reason: { code: "Stalled", message: "The connection stalled." } })]
  const korrid = brain({ all: () => world, game: () => world })
  const downloads = createGameDownloads(korrid.client)
  await downloads.refresh(["mm"])
  expect(downloads.getFacts().get("mm")).toMatchObject({ needs: true, active: false, failure: "The connection stalled." })
  downloads.dispose()
})
