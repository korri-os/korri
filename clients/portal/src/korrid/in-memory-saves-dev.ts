/**
 * Browser-dev seeds for save sync (ADR 0014), so every portal state can be
 * reached in `portal-dev` without a second device. Pick one with `?saves=`:
 *
 *   (none)           Skate 3 split between this browser and zao; Neverball
 *                    synced, with an interrupted snapshot in its history
 *   in-use           Neverball is in use on zao
 *   mismatch         Neverball's latest save came from another release
 *   slow             no device answers within the 3 s limit
 *   unavailable      korrid answers SavesUnavailable, as before the lifecycle
 *   history-error    the history read fails
 *   restore-missing  a restore's older files are on no device in reach
 *   choose-fail      a pick is refused while a session is live
 *
 * Shapes are the generated wire types; the content is made up.
 */
import { PeerListState, type Game, type SaveHistoryEntry } from "@contracts/generated/korrid"
import type { InMemoryKorridClientConfig } from "./client"
import type { InMemorySaveGame, InMemorySavesConfig } from "./in-memory-saves"

const BROWSER = "browser-device"
const ZAO = "zao-device"
const HOUR = 3_600_000

const local = { label: "browser", isLocal: true, devicePublicKey: BROWSER }
const games: readonly Game[] = [
  { id: "skate3", title: "Skate 3", supportsRunnerSelection: false, source: local },
  { id: "neverball", title: "Neverball", supportsRunnerSelection: false, source: local },
]

function snapshot(
  snapshotId: string,
  gameId: string,
  device: string,
  hoursAgo: number,
  parents: readonly string[],
  extra: Partial<SaveHistoryEntry["record"]> = {},
): SaveHistoryEntry {
  return {
    snapshotId,
    record: {
      parents: [...parents],
      gameId,
      runnerId: "@korri:retroarch",
      device,
      endedAt: new Date(Date.now() - hoursAgo * HOUR).toISOString(),
      durationSeconds: 1800,
      interrupted: false,
      locations: [],
      ...extra,
    },
  }
}

function seed(): Record<string, InMemorySaveGame> {
  return {
    skate3: {
      records: [
        snapshot("sha256:s1", "skate3", BROWSER, 50, []),
        snapshot("sha256:s2", "skate3", BROWSER, 2, ["sha256:s1"], { durationSeconds: 4320 }),
        snapshot("sha256:s3", "skate3", ZAO, 27, ["sha256:s1"], { durationSeconds: 1512 }),
      ],
      heads: ["sha256:s2", "sha256:s3"],
      current: "sha256:s1",
    },
    neverball: {
      records: [
        snapshot("sha256:n1", "neverball", ZAO, 200, []),
        snapshot("sha256:n2", "neverball", BROWSER, 120, ["sha256:n1"], { interrupted: true }),
        snapshot("sha256:n3", "neverball", BROWSER, 5, ["sha256:n2"], { durationSeconds: 40 }),
      ],
      heads: ["sha256:n3"],
      current: "sha256:n3",
    },
  }
}

export function inMemorySavesDevConfig(scenario: string | null): InMemoryKorridClientConfig {
  const common: InMemoryKorridClientConfig = {
    games,
    peerList: {
      _tag: "Ok",
      payload: { peers: [{ devicePublicKey: ZAO, label: "zao", state: PeerListState.Ready, updatedAt: 0 }] },
    },
  }
  if (scenario === "unavailable") return common
  const seeded = seed()
  const neverball = seeded.neverball!
  let saves: InMemorySavesConfig = { localDevice: BROWSER, games: seeded }
  switch (scenario) {
    case "in-use":
      saves = { ...saves, games: { ...seeded, neverball: { ...neverball, inUse: [{ device: ZAO, since: Date.now() - 0.3 * HOUR }] } } }
      break
    case "mismatch":
      saves = {
        ...saves,
        games: {
          ...seeded,
          neverball: {
            ...neverball,
            warnings: ["The latest save came from Neverball 1.6. You are starting Neverball 1.7. It may not load."],
          },
        },
      }
      break
    case "slow":
      saves = {
        ...saves,
        games: Object.fromEntries(Object.entries(seeded).map(([id, game]) => [id, { ...game, checkDelayMs: 5000 }])),
      }
      break
    case "history-error":
      saves = { ...saves, failures: { history: { code: "SavesUnavailable", message: "save history could not be read" } } }
      break
    case "restore-missing":
      saves = { ...saves, failures: { restore: { code: "SavePayloadUnavailable", message: "payload unavailable" } } }
      break
    case "choose-fail":
      saves = { ...saves, failures: { choose: { code: "ActiveSessionConflict", message: "session is live" } } }
      break
    default:
      break
  }
  return { ...common, saves }
}
