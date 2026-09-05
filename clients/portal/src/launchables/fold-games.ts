import type {
  Game,
  GameIdentity,
  LocalGame,
  PlayStats,
} from "@contracts/generated/korrid"

export type PortalLocalGame = LocalGame & { readonly coverArtUrl?: string }

export type PortalGameCopy =
  | { readonly kind: "local"; readonly game: PortalLocalGame }
  | { readonly kind: "remote"; readonly game: Game }

export interface FoldedPortalGame {
  readonly primary: PortalGameCopy
  readonly alternatives: readonly PortalGameCopy[]
}

/**
 * Port of legacy's catalog fold at the current portal seam. Games only fold
 * when a host supplied one unambiguous release identity. The local copy wins;
 * remote-only groups use stable host/id ordering.
 */
export function foldGameCopies(
  localGames: readonly PortalLocalGame[],
  remoteGames: readonly Game[],
): readonly FoldedPortalGame[] {
  const copies: readonly PortalGameCopy[] = [
    ...localGames.map(game => ({ kind: "local" as const, game })),
    ...remoteGames.map(game => ({ kind: "remote" as const, game })),
  ]
  const groups = new Map<string, PortalGameCopy[]>()

  copies.forEach((copy, index) => {
    const key = identityKey(copy.game.identity) ?? `tagless:${index}`
    const group = groups.get(key) ?? []
    group.push(copy)
    groups.set(key, group)
  })

  return [...groups.values()].map(group => {
    const primary = choosePrimary(group)
    return {
      primary,
      alternatives: group
        .filter(copy => copy !== primary)
        .sort(compareCopies),
    }
  })
}

/**
 * Play stats for one folded game. Play history is game-centric, not
 * host-centric: the newest lastPlayed across every copy wins, counts and
 * playtime sum. Returns undefined when no copy has been played, so Recent
 * can keep treating absence as the never-played signal.
 */
export function mergePlayStats(
  copies: readonly PortalGameCopy[],
): PlayStats | undefined {
  const played = copies
    .map(copy => copy.game.playStats)
    .filter((stats): stats is PlayStats => stats !== undefined)
  if (played.length === 0) return undefined
  if (played.length === 1) return played[0]

  let lastPlayed: string | undefined
  let playCount = 0
  let totalPlaytimeSeconds = 0
  for (const stats of played) {
    if (
      stats.lastPlayed !== undefined &&
      (lastPlayed === undefined || stats.lastPlayed > lastPlayed)
    ) {
      lastPlayed = stats.lastPlayed
    }
    playCount += stats.playCount
    totalPlaytimeSeconds += stats.totalPlaytimeSeconds
  }
  return {
    ...(lastPlayed === undefined ? {} : { lastPlayed }),
    playCount,
    totalPlaytimeSeconds,
  }
}

function identityKey(identity: GameIdentity | undefined): string | undefined {
  if (identity === undefined) return undefined
  return identity.kind === "hash"
    ? `hash:${identity.value}`
    : `provider:${identity.value.provider}\0${identity.value.ref}`
}

function choosePrimary(group: readonly PortalGameCopy[]): PortalGameCopy {
  return (
    group.find(copy => copy.kind === "local") ??
    [...group].sort(compareCopies)[0]!
  )
}

function compareCopies(left: PortalGameCopy, right: PortalGameCopy): number {
  if (left.kind !== right.kind) return left.kind === "local" ? -1 : 1
  if (left.kind === "local" && right.kind === "local") {
    return left.game.id.localeCompare(right.game.id)
  }
  if (left.kind === "remote" && right.kind === "remote") {
    return (
      (left.game.host ?? "").localeCompare(right.game.host ?? "") ||
      left.game.id.localeCompare(right.game.id)
    )
  }
  return 0
}
