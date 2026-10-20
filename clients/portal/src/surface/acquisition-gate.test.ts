import { expect, test } from "bun:test"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import type { AcquisitionOutcome, ArtifactStatus } from "../korrid/acquisition-wire"
import {
  confirmDetail,
  createAcquisitionGate,
  downloadDetail,
  formatBytes,
  formatEta,
} from "./acquisition-gate"

const rootfs = (state: ArtifactStatus["state"]): ArtifactStatus => ({
  id: "@korri:fex/rootfs", title: "FEX rootfs", version: "archlinux-2026-01-08", bytesTotal: 1_160_000_000, state,
})
const proton = (state: ArtifactStatus["state"]): ArtifactStatus => ({
  id: "@korri:proton/cachyos", title: "Proton-CachyOS", version: "11.0", bytesTotal: 340_000_000, state,
})
const ready = { _tag: "Ready", path: "/cache/x" } as const
const missing = { _tag: "Missing" } as const
const ok = (artifacts: ArtifactStatus[]): AcquisitionOutcome => ({ _tag: "Ok", payload: { artifacts } })
const game = { id: "steam:1", surfaceId: "game:steam:1", title: "Hollow Knight" }
const flush = () => new Promise(resolve => setTimeout(resolve, 0))
const settle = async (times = 4) => { for (let i = 0; i < times; i++) await flush() }

/** A korrid whose answers are a script: each call shifts the next answer,
 * and the last answer stays current once the script runs out, as a polled
 * daemon's state would. */
function scripted() {
  const calls: { method: string; args: unknown }[] = []
  const answers = { status: [] as AcquisitionOutcome[], start: [] as AcquisitionOutcome[], cancel: [] as AcquisitionOutcome[] }
  const last: Partial<Record<keyof typeof answers, AcquisitionOutcome>> = {}
  const next = (method: keyof typeof answers, args: unknown): AcquisitionOutcome => {
    calls.push({ method, args })
    const answer = answers[method].shift() ?? last[method]
    if (!answer) throw new Error(`no scripted answer for ${method}`)
    last[method] = answer
    return answer
  }
  /** A start the test holds back: korrid has not answered yet. */
  let holdStart: Promise<void> | undefined
  return {
    calls,
    answers,
    holdStart: (promise: Promise<void>) => { holdStart = promise },
    client: {
      acquisitionStatus: async (gameId?: string, runnerId?: string) => next("status", runnerId === undefined ? gameId : [gameId, runnerId]),
      acquisitionStart: async (ids: readonly string[]) => {
        calls.push({ method: "start", args: [...ids] })
        await holdStart
        const answer = answers.start.shift() ?? last.start
        if (!answer) throw new Error("no scripted answer for start")
        last.start = answer
        return answer
      },
      acquisitionCancel: async (id: string) => next("cancel", id),
    },
  }
}

function watch(gate: ReturnType<typeof createAcquisitionGate>) {
  const seen: (SurfaceStatus | undefined)[] = []
  gate.subscribe(() => seen.push(gate.getStatus()))
  return seen
}

const actionId = (status: SurfaceStatus | undefined, label: string) => {
  if (status?._tag !== "Confirm" && status?._tag !== "Busy") throw new Error(`no actions on ${status?._tag}`)
  const action = (status.actions ?? []).find(candidate => candidate.label === label)
  if (!action) throw new Error(`no ${label} action`)
  return action.id
}

test("formatting: sizes, rates and time left read the way the contract shows them", () => {
  expect(formatBytes(612_000_000)).toBe("612 MB")
  expect(formatBytes(1_160_000_000)).toBe("1.2 GB")
  expect(formatBytes(1_500_000_000)).toBe("1.5 GB")
  expect(formatBytes(2_000_000_000)).toBe("2 GB")
  expect(formatBytes(48_000)).toBe("48 KB")
  expect(formatBytes(900)).toBe("900 B")
  expect(formatEta(90)).toBe("1:30")
  expect(formatEta(5)).toBe("0:05")
  expect(formatEta(3723)).toBe("1:02:03")
  expect(downloadDetail({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 612_000_000, bytesTotal: 1_200_000_000, bytesPerSecond: 6_400_000 }))
    .toBe("612 MB of 1.2 GB · 6.4 MB/s · 1:32 left")
  // Unknown parts are left out, never invented.
  expect(downloadDetail({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 612_000_000, bytesTotal: 1_200_000_000 }))
    .toBe("612 MB of 1.2 GB")
  expect(downloadDetail({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 612_000_000, bytesPerSecond: 6_400_000 }))
    .toBe("612 MB so far · 6.4 MB/s")
  expect(confirmDetail([rootfs(missing), proton(missing)])).toBe("FEX rootfs and Proton-CachyOS, 1.5 GB")
  expect(confirmDetail([rootfs(missing)])).toBe("FEX rootfs, 1.2 GB")
  expect(confirmDetail([rootfs(missing), { ...proton(missing), bytesTotal: undefined }])).toBe("FEX rootfs and Proton-CachyOS, size unknown")
})

test("every artifact Ready: the gate steps aside at once and the launch proceeds", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(ready), proton(ready)]), ok([rootfs(ready), proton(ready)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  const seen = watch(gate)
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Starting Hollow Knight…", gameId: "game:steam:1" })
  await settle()
  expect(proceeded).toBe(1)
  expect(gate.getStatus()).toBeUndefined()
  expect(korrid.calls[0]).toEqual({ method: "status", args: "steam:1" })
  expect(seen.at(-1)).toBeUndefined()
})

test("Missing -> Confirm -> Download -> Downloading with progress -> Unpacking -> Ready -> the existing start", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  const confirm = gate.getStatus()
  expect(confirm).toMatchObject({
    _tag: "Confirm", kicker: "Needs a download", detail: "FEX rootfs and Proton-CachyOS, 1.5 GB", gameId: "game:steam:1",
    actions: [{ label: "Download", enabled: true }, { label: "Cancel", enabled: true }],
  })

  const tenPercent = ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 116_000_000, bytesTotal: 1_160_000_000, bytesPerSecond: 6_400_000 }),
    proton(missing),
  ])
  korrid.answers.start.push(tenPercent)
  korrid.answers.status.push(tenPercent)
  expect(gate.act(actionId(confirm, "Download"))).toBe(true)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Starting download…", actions: [{ label: "Cancel" }] })
  await flush()
  expect(korrid.calls.find(call => call.method === "start")).toEqual({ method: "start", args: ["@korri:fex/rootfs", "@korri:proton/cachyos"] })
  await settle()
  const downloading = gate.getStatus()
  expect(downloading).toMatchObject({
    _tag: "Busy", kicker: "Downloading FEX rootfs",
    progress: { done: 116_000_000, total: 1_160_000_000 },
    detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
    actions: [{ label: "Cancel", enabled: true }], gameId: "game:steam:1",
  })

  // Polled at the configured cadence; each answer becomes the next Busy.
  korrid.answers.status.push(ok([rootfs({ _tag: "Unpacking" }), proton(missing)]))
  await settle(8)
  // Unpacking: a position of nothing and no known end, and nothing to cancel.
  expect(gate.getStatus()).toEqual({ _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 }, gameId: "game:steam:1" })

  korrid.answers.status.push(ok([rootfs(ready), proton({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 0, bytesTotal: 340_000_000 })]))
  await settle(8)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Downloading Proton-CachyOS", progress: { done: 0, total: 340_000_000 }, detail: "0 B of 340 MB" })

  korrid.answers.status.push(ok([rootfs(ready), proton(ready)]))
  await settle(8)
  expect(proceeded).toBe(1)
  expect(gate.getStatus()).toBeUndefined()
  expect(korrid.calls.filter(call => call.method === "status" && call.args === "steam:1").length).toBeGreaterThanOrEqual(4)
})

test("Failed -> Problem with korrid's words -> Retry starts again and resumes observing", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  const begun = ok([rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 0, bytesTotal: 1_160_000_000 })])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Downloading FEX rootfs" })
  korrid.answers.status.push(ok([rootfs({ _tag: "Failed", reason: { code: "Stalled", message: "The connection stalled. Check the network and try again." } })]))
  await settle(8)
  expect(gate.getStatus()).toEqual({
    _tag: "Problem", kicker: "Couldn't download FEX rootfs",
    reason: "The connection stalled. Check the network and try again.",
    canRetry: true, gameId: "game:steam:1", gameTitle: "Hollow Knight",
  })
  // No more polling while failed.
  const statusCalls = korrid.calls.filter(call => call.method === "status").length
  await settle(8)
  expect(korrid.calls.filter(call => call.method === "status").length).toBe(statusCalls)

  const resumed = ok([rootfs({ _tag: "Downloading", file: 3, fileCount: 11, bytesDone: 700_000_000, bytesTotal: 1_160_000_000, bytesPerSecond: 5_000_000 })])
  korrid.answers.start.push(resumed)
  korrid.answers.status.push(resumed)
  expect(gate.retry()).toBe(true)
  await settle()
  expect(korrid.calls.filter(call => call.method === "start").at(-1)).toEqual({ method: "start", args: ["@korri:fex/rootfs"] })
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Downloading FEX rootfs", progress: { done: 700_000_000, total: 1_160_000_000 } })
  korrid.answers.status.push(ok([rootfs(ready)]), ok([rootfs(ready)]))
  await settle(8)
  expect(proceeded).toBe(1)
  expect(gate.getStatus()).toBeUndefined()
})

test("Dismiss on a Problem closes the gate without starting anything", async () => {
  const korrid = scripted()
  korrid.answers.status.push({ _tag: "Err", payload: { code: "BrainUnreachable", message: "korrid did not answer." } })
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  expect(gate.getStatus()).toMatchObject({ _tag: "Problem", kicker: "Couldn't check for downloads", reason: "korrid did not answer.", canRetry: true })
  korrid.answers.status.push(ok([]))
  expect(gate.dismiss()).toBe(true)
  expect(gate.getStatus()).toBeUndefined()
  expect(proceeded).toBe(0)
  expect(korrid.calls.filter(call => call.method === "start")).toEqual([])
})

test("Cancel on Confirm closes without an RPC; Cancel while downloading calls cancel and keeps the launch unstarted", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  expect(gate.act(actionId(gate.getStatus(), "Cancel"))).toBe(true)
  expect(gate.getStatus()).toBeUndefined()
  expect(korrid.calls.filter(call => call.method !== "status")).toEqual([])

  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  gate.launch(game, () => proceeded++)
  await settle()
  const begun = ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 10, bytesTotal: 1_160_000_000 }), proton(missing),
  ])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  korrid.answers.cancel.push(ok([rootfs(missing)]))
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  expect(gate.act(actionId(gate.getStatus(), "Cancel"))).toBe(true)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Cancelling download…" })
  await settle(8)
  // Only the live download is cancelled; a Missing artifact has nothing to stop.
  expect(korrid.calls.filter(call => call.method === "cancel").map(call => call.args)).toEqual(["@korri:fex/rootfs"])
  expect(gate.getStatus()).toBeUndefined()
  expect(proceeded).toBe(0)
})

test("leaving a Problem while another artifact still downloads cancels exactly that one", async () => {
  for (const leave of ["dismiss", "back"] as const) {
    const korrid = scripted()
    korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
    const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
    let proceeded = 0
    let abandoned = 0
    gate.launch(game, () => proceeded++, () => abandoned++)
    await settle()
    const mixed = ok([
      rootfs({ _tag: "Failed", reason: { code: "Stalled", message: "The connection stalled." } }),
      proton({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 10, bytesTotal: 340_000_000 }),
    ])
    korrid.answers.start.push(mixed)
    korrid.answers.status.push(mixed)
    gate.act(actionId(gate.getStatus(), "Download"))
    await settle()
    expect(gate.getStatus()).toMatchObject({ _tag: "Problem", kicker: "Couldn't download FEX rootfs", reason: "The connection stalled." })
    korrid.answers.cancel.push(ok([proton(missing)]))
    korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
    expect(leave === "dismiss" ? gate.dismiss() : gate.back()).toBe(true)
    await settle(8)
    expect(korrid.calls.filter(call => call.method === "cancel").map(call => call.args)).toEqual(["@korri:proton/cachyos"])
    expect(gate.getStatus()).toBeUndefined()
    expect(proceeded).toBe(0)
    expect(abandoned).toBe(1)
  }
})

test("joining work korrid already has under way, then asking for what is still Missing", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs({ _tag: "Unpacking" }), proton(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  expect(gate.getStatus()).toEqual({ _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 }, gameId: "game:steam:1" })
  korrid.answers.status.push(ok([rootfs(ready), proton(missing)]))
  await settle(8)
  // The found work is done; Proton still needs consent and a size.
  const confirm = gate.getStatus()
  expect(confirm).toMatchObject({ _tag: "Confirm", kicker: "Needs a download", detail: "Proton-CachyOS, 340 MB" })
  expect(korrid.calls.filter(call => call.method === "start")).toEqual([])
  const begun = ok([rootfs(ready), proton({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 0, bytesTotal: 340_000_000 })])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(confirm, "Download"))
  await settle()
  expect(korrid.calls.filter(call => call.method === "start").map(call => call.args)).toEqual([["@korri:proton/cachyos"]])
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Downloading Proton-CachyOS" })
  korrid.answers.status.push(ok([rootfs(ready), proton(ready)]))
  await settle(8)
  expect(proceeded).toBe(1)
  expect(gate.getStatus()).toBeUndefined()
})

test("until korrid answers a start, the gate waits; a Missing it reports after is not queued, so the question returns", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  gate.launch(game, () => {})
  await settle()
  let release!: () => void
  korrid.holdStart(new Promise<void>(resolve => { release = resolve }))
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle(8)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Starting download…", actions: [{ label: "Cancel" }] })
  // korrid makes Downloading the moment it accepts work: a Missing answer
  // means nothing was started, and nothing is started again without asking.
  korrid.answers.start.push(ok([rootfs(missing)]))
  release()
  await settle(8)
  expect(gate.getStatus()).toMatchObject({ _tag: "Confirm", detail: "FEX rootfs, 1.2 GB" })
  expect(korrid.calls.filter(call => call.method === "start")).toHaveLength(1)
})

test("Cancel while the start is still pending is kept, then stops exactly what the start reports as Downloading", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let abandoned = 0
  gate.launch(game, () => {}, () => abandoned++)
  await settle()
  let release!: () => void
  korrid.holdStart(new Promise<void>(resolve => { release = resolve }))
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  expect(gate.act(actionId(gate.getStatus(), "Cancel"))).toBe(true)
  const cancelling = gate.getStatus()
  expect(cancelling).toMatchObject({ _tag: "Busy", kicker: "Cancelling download…" })
  expect(cancelling?._tag === "Busy" ? cancelling.actions : "no busy").toBeUndefined()
  // Nothing to cancel yet: korrid has not said what it started.
  await settle(4)
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Cancelling download…" })
  korrid.answers.start.push(ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 0, bytesTotal: 1_160_000_000 }),
    proton({ _tag: "Unpacking" }),
  ]))
  korrid.answers.cancel.push(ok([rootfs(missing)]))
  korrid.answers.status.push(ok([rootfs(missing), proton({ _tag: "Unpacking" })]))
  release()
  await settle(8)
  // The download korrid began is stopped; the unpacking is never cancelled.
  expect(korrid.calls.filter(call => call.method === "cancel").map(call => call.args)).toEqual(["@korri:fex/rootfs"])
  expect(gate.getStatus()).toBeUndefined()
  expect(abandoned).toBe(1)
})

test("Cancel while the start is pending and the start then fails: nothing to stop, the gate closes", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let abandoned = 0
  gate.launch(game, () => {}, () => abandoned++)
  await settle()
  let release!: () => void
  korrid.holdStart(new Promise<void>(resolve => { release = resolve }))
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  gate.act(actionId(gate.getStatus(), "Cancel"))
  korrid.answers.start.push({ _tag: "Err", payload: { code: "BrainUnreachable", message: "korrid did not answer." } })
  korrid.answers.status.push(ok([rootfs(missing)]))
  release()
  await settle(8)
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
  expect(gate.getStatus()).toBeUndefined()
  expect(abandoned).toBe(1)
})

test("an acknowledged start later observed Missing (korrid restarted) returns to the question, with no second start", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let proceeded = 0
  gate.launch(game, () => proceeded++)
  await settle()
  const begun = ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 5_000_000, bytesTotal: 1_160_000_000 }),
    proton({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 0, bytesTotal: 340_000_000 }),
  ])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle(6)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Downloading FEX rootfs" })
  korrid.answers.status.push(ok([rootfs(missing), proton(missing)]))
  await settle(8)
  expect(gate.getStatus()).toMatchObject({ _tag: "Confirm", detail: "FEX rootfs and Proton-CachyOS, 1.5 GB" })
  const starts = korrid.calls.filter(call => call.method === "start").length
  await settle(8)
  expect(korrid.calls.filter(call => call.method === "start")).toHaveLength(starts)
  expect(starts).toBe(1)
  expect(proceeded).toBe(0)
})

test("the runner the user chose goes with the status read, and the gate reports an abandoned launch", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  let abandoned = 0
  gate.launch({ ...game, runnerId: "retroarch/mgba" }, () => {}, () => abandoned++)
  await settle()
  expect(korrid.calls[0]).toEqual({ method: "status", args: ["steam:1", "retroarch/mgba"] })
  gate.act(actionId(gate.getStatus(), "Cancel"))
  expect(abandoned).toBe(1)
  expect(gate.getStatus()).toBeUndefined()
})

test("Back is Cancel on Confirm and while downloading, nothing while unpacking, Dismiss on a Problem", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  expect(gate.back()).toBe(false)
  gate.launch(game, () => {})
  await settle()
  expect(gate.getStatus()?._tag).toBe("Confirm")
  expect(gate.back()).toBe(true)
  expect(gate.getStatus()).toBeUndefined()

  korrid.answers.status.push(ok([rootfs({ _tag: "Unpacking" })]))
  gate.launch(game, () => {})
  await settle()
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 } })
  expect(gate.back()).toBe(true)
  expect(gate.getStatus()).toMatchObject({ _tag: "Busy", kicker: "Unpacking FEX rootfs" })
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
  gate.dispose()
})

test("a stale action id, from an earlier generation, sends nothing", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  gate.launch(game, () => {})
  await settle()
  const stale = actionId(gate.getStatus(), "Download")
  gate.back()
  korrid.answers.status.push(ok([rootfs(missing)]))
  gate.launch(game, () => {})
  await settle()
  expect(gate.act(stale)).toBe(true)
  await settle()
  expect(korrid.calls.filter(call => call.method === "start")).toEqual([])
  expect(gate.getStatus()?._tag).toBe("Confirm")
})

test("a download already queued or under way when the gate opens goes to the front and is never cancelled", async () => {
  const korrid = scripted()
  // The person asked for the download from the game; the rootfs is running,
  // Proton waits its turn behind another game's download.
  const background = ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 50_000_000, bytesTotal: 1_160_000_000 }),
    proton({ _tag: "Queued" }),
  ])
  korrid.answers.status.push(background)
  korrid.answers.start.push(ok([proton({ _tag: "Queued" })]))
  let settled = 0
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1, onSettled: () => settled++ })
  let proceeded = 0
  let abandoned = 0
  gate.launch(game, () => proceeded++, () => abandoned++)
  await settle()
  // No question: the person already said yes. The queued one is moved up.
  expect(korrid.calls.filter(call => call.method === "start").map(call => call.args)).toEqual([["@korri:proton/cachyos"]])
  const busy = gate.getStatus()
  expect(busy).toMatchObject({ _tag: "Busy", kicker: "Downloading FEX rootfs", actions: [{ label: "Play later", enabled: true }] })
  // Leaving closes the launch only; the download goes on in the background.
  expect(gate.act(actionId(busy, "Play later"))).toBe(true)
  await settle()
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
  expect(gate.getStatus()).toBeUndefined()
  expect(abandoned).toBe(1)
  expect(proceeded).toBe(0)
  expect(settled).toBe(1)

  // Back does the same.
  korrid.answers.status.push(background)
  gate.launch(game, () => proceeded++, () => abandoned++)
  await settle()
  expect(gate.back()).toBe(true)
  await settle()
  expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
  expect(abandoned).toBe(2)
})

test("a Problem left while a pre-existing download still runs cancels only what the gate started", async () => {
  for (const leave of ["dismiss", "back"] as const) {
    const korrid = scripted()
    // The rootfs was already downloading in the background; Proton was not.
    korrid.answers.status.push(ok([
      rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 10, bytesTotal: 1_160_000_000 }),
      proton(missing),
    ]))
    const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
    gate.launch(game, () => {})
    await settle()
    korrid.answers.status.push(ok([rootfs(ready), proton(missing)]))
    await settle(8)
    expect(gate.getStatus()).toMatchObject({ _tag: "Confirm", detail: "Proton-CachyOS, 340 MB" })
    // A second background download of the rootfs (a repair, a new version)
    // shows up while the gate's own Proton download fails.
    const mixed = ok([
      rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 20, bytesTotal: 1_160_000_000 }),
      proton({ _tag: "Failed", reason: { code: "Stalled", message: "The connection stalled." } }),
    ])
    korrid.answers.start.push(mixed)
    korrid.answers.status.push(mixed)
    gate.act(actionId(gate.getStatus(), "Download"))
    await settle()
    expect(gate.getStatus()).toMatchObject({ _tag: "Problem", kicker: "Couldn't download Proton-CachyOS" })
    expect(leave === "dismiss" ? gate.dismiss() : gate.back()).toBe(true)
    await settle(8)
    expect(korrid.calls.filter(call => call.method === "cancel")).toEqual([])
    expect(gate.getStatus()).toBeUndefined()
  }
})

test("Cancel stops what the gate started, queued as well as downloading, and nothing it found under way", async () => {
  const korrid = scripted()
  const dlc = (state: ArtifactStatus["state"]): ArtifactStatus => ({ id: "@game:dlc", title: "Extra levels", version: "1", bytesTotal: 20_000_000, state })
  // The rootfs was downloading before the gate opened; Proton and the game's
  // own files are missing.
  korrid.answers.status.push(ok([rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 10, bytesTotal: 1_160_000_000 }), proton(missing), dlc(missing)]))
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1 })
  gate.launch(game, () => {})
  await settle()
  korrid.answers.status.push(ok([rootfs(ready), proton(missing), dlc(missing)]))
  await settle(8)
  // Now the rootfs is downloading again for someone else, which the gate
  // first sees in its start's answer.
  const begun = ok([
    rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 10, bytesTotal: 1_160_000_000 }),
    proton({ _tag: "Downloading", file: 1, fileCount: 1, bytesDone: 5, bytesTotal: 340_000_000 }),
    dlc({ _tag: "Queued" }),
  ])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  expect(korrid.calls.filter(call => call.method === "start").map(call => call.args)).toEqual([["@korri:proton/cachyos", "@game:dlc"]])
  korrid.answers.cancel.push(ok([proton(missing)]), ok([dlc(missing)]))
  gate.act(actionId(gate.getStatus(), "Cancel"))
  await settle(8)
  expect(korrid.calls.filter(call => call.method === "cancel").map(call => call.args)).toEqual(["@korri:proton/cachyos", "@game:dlc"])
  expect(gate.getStatus()).toBeUndefined()
})

test("Cancelling says the partial download is deleted, and the host hears that the gate settled", async () => {
  const korrid = scripted()
  korrid.answers.status.push(ok([rootfs(missing)]))
  let settled = 0
  const gate = createAcquisitionGate(korrid.client, { pollIntervalMs: 1, onSettled: () => settled++ })
  gate.launch(game, () => {})
  await settle()
  const begun = ok([rootfs({ _tag: "Downloading", file: 1, fileCount: 11, bytesDone: 10, bytesTotal: 1_160_000_000 })])
  korrid.answers.start.push(begun)
  korrid.answers.status.push(begun)
  gate.act(actionId(gate.getStatus(), "Download"))
  await settle()
  let release!: () => void
  const held = new Promise<void>(resolve => { release = resolve })
  const cancel = korrid.client.acquisitionCancel
  korrid.client.acquisitionCancel = async (id: string) => { await held; return cancel(id) }
  korrid.answers.cancel.push(ok([rootfs(missing)]))
  gate.act(actionId(gate.getStatus(), "Cancel"))
  expect(gate.getStatus()).toEqual({ _tag: "Busy", kicker: "Cancelling download…", detail: "Deleting the partial download", gameId: "game:steam:1" })
  release()
  await settle(8)
  expect(gate.getStatus()).toBeUndefined()
  expect(settled).toBe(1)
})
