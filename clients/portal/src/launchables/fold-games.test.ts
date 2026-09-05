import { describe, expect, it } from "bun:test"
import type { Game, GameIdentity, LocalGame } from "@contracts/generated/korrid"
import { foldGameCopies, mergePlayStats } from "./fold-games"

const hash = (value: string): GameIdentity => ({ kind: "hash", value })
const provider = (ref: string): GameIdentity => ({
  kind: "provider",
  value: { provider: "@korri:store", ref },
})
const local = (id: string, identity?: GameIdentity): LocalGame => ({
  id,
  title: id,
  system: "Game Boy Advance",
  ...(identity === undefined ? {} : { identity }),
})
const remote = (
  id: string,
  host: string,
  identity?: GameIdentity,
): Game => ({
  id,
  title: id,
  host,
  source: { label: host, isLocal: false },
  ...(identity === undefined ? {} : { identity }),
})

describe("foldGameCopies", () => {
  it("folds matching local and remote hashes with the local copy first", () => {
    const identity = hash("sha256:wario")
    const folded = foldGameCopies(
      [local("wl4", identity)],
      [remote("wl4", "zao", identity)],
    )

    expect(folded).toEqual([
      {
        primary: { kind: "local", game: local("wl4", identity) },
        alternatives: [
          { kind: "remote", game: remote("wl4", "zao", identity) },
        ],
      },
    ])
  })

  it("folds provider identities only when provider and ref both match", () => {
    const folded = foldGameCopies([], [
      remote("a", "zao", provider("same")),
      remote("b", "aka", provider("same")),
      remote("c", "aka", provider("different")),
      remote("d", "aka", {
        kind: "provider",
        value: { provider: "@korri:other", ref: "same" },
      }),
    ])

    expect(folded).toHaveLength(3)
    expect(folded[0]?.primary).toEqual({
      kind: "remote",
      game: remote("b", "aka", provider("same")),
    })
    expect(folded[0]?.alternatives).toEqual([
      { kind: "remote", game: remote("a", "zao", provider("same")) },
    ])
  })

  it("never folds games without an identity", () => {
    expect(foldGameCopies([local("wl4")], [remote("wl4", "zao")])).toHaveLength(2)
  })
})

describe("mergePlayStats", () => {
  it("is absent when no copy has been played", () => {
    expect(
      mergePlayStats([
        { kind: "local", game: local("wl4") },
        { kind: "remote", game: remote("wl4", "zao") },
      ]),
    ).toBeUndefined()
  })

  it("preserves a single played copy exactly", () => {
    const stats = {
      lastPlayed: "2026-09-01T10:00:00Z",
      playCount: 3,
      totalPlaytimeSeconds: 900,
    }
    expect(
      mergePlayStats([
        { kind: "local", game: local("wl4") },
        { kind: "remote", game: { ...remote("wl4", "zao"), playStats: stats } },
      ]),
    ).toEqual(stats)
  })

  it("takes the newest lastPlayed and sums counts across copies", () => {
    expect(
      mergePlayStats([
        {
          kind: "local",
          game: {
            ...local("wl4"),
            playStats: {
              lastPlayed: "2026-08-15T10:00:00Z",
              playCount: 2,
              totalPlaytimeSeconds: 600,
            },
          },
        },
        {
          kind: "remote",
          game: {
            ...remote("wl4", "zao"),
            playStats: {
              lastPlayed: "2026-09-04T18:00:00Z",
              playCount: 1,
              totalPlaytimeSeconds: 1200,
            },
          },
        },
      ]),
    ).toEqual({
      lastPlayed: "2026-09-04T18:00:00Z",
      playCount: 3,
      totalPlaytimeSeconds: 1800,
    })
  })
})
