import { describe, expect, it } from "bun:test"
import {
  PluginInstalledState,
  PluginRepositoryAddResult,
  PluginRepositoryRemoveResult,
} from "@contracts/generated/korrid"
import { createHttpKorridClient, createInMemoryKorridClient } from "./client"

const OFFICIAL = "https://official.example/catalog.json"
const COMMUNITY = "https://community.example/catalog.json"

const ok = <T>(outcome: { _tag: "Ok", payload: T } | { _tag: "Err", payload: unknown }): T => {
  if (outcome._tag !== "Ok") throw new Error(`expected Ok, got ${JSON.stringify(outcome)}`)
  return outcome.payload
}

describe("plugin repositories over the live client", () => {
  it("sends each method's exact wire tag and payload", async () => {
    const bodies: unknown[] = []
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        const body = await request.json() as { _tag: string }
        bodies.push(body)
        return Response.json({ _tag: body._tag, outcome: { _tag: "Ok", payload: {} } })
      },
    })
    try {
      const client = createHttpKorridClient(server.url.origin, "capability")
      await client.pluginRepositories()
      await client.addPluginRepository(COMMUNITY)
      await client.removePluginRepository(COMMUNITY)
      await client.pluginCatalog(OFFICIAL)
      await client.inspectPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1")
      await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1", "digest")
      await client.setPluginEnabled("@korri:sunshine", true)
      expect(bodies).toEqual([
        { _tag: "system.plugins.repositories", payload: {} },
        { _tag: "system.plugins.repository.add", payload: { url: COMMUNITY } },
        { _tag: "system.plugins.repository.remove", payload: { url: COMMUNITY } },
        { _tag: "system.plugins.catalog", payload: { url: OFFICIAL } },
        { _tag: "system.plugins.inspect", payload: { url: OFFICIAL, pluginId: "@korri:sunshine", releaseVersion: "2026.10.1" } },
        { _tag: "system.plugins.install", payload: { url: OFFICIAL, pluginId: "@korri:sunshine", releaseVersion: "2026.10.1", approval: "digest" } },
        { _tag: "system.plugins.setEnabled", payload: { id: "@korri:sunshine", enabled: true } },
      ])
    } finally {
      server.stop(true)
    }
  })

  it("reports a refused portal permission without throwing", async () => {
    const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(null, { status: 403 }) })
    try {
      const client = createHttpKorridClient(server.url.origin, "capability")
      expect(await client.setPluginEnabled("@korri:sunshine", true)).toMatchObject({
        _tag: "Err", payload: { code: "PermissionDenied" },
      })
    } finally {
      server.stop(true)
    }
  })
})

describe("plugin repositories in the in-memory brain", () => {
  it("seeds the official repository, an added one, and an install from a removed one", async () => {
    const client = createInMemoryKorridClient()
    const repositories = ok(await client.pluginRepositories())
    expect(repositories.official).toBe(OFFICIAL)
    expect(repositories.userAdded).toEqual([COMMUNITY])
    const removed = repositories.repositoryInstalls.filter(install =>
      install.sourceUrl !== repositories.official && !repositories.userAdded.includes(install.sourceUrl))
    expect(removed).toEqual([{ id: "@korri:retro-clock", sourceUrl: "https://old.example/catalog.json", releaseVersion: "2.0.0", enabled: false }])
    // A removed repository's install still turns on and off.
    expect(ok(await client.setPluginEnabled("@korri:retro-clock", true))).toEqual({})
    expect(ok(await client.pluginRepositories()).repositoryInstalls.find(item => item.id === "@korri:retro-clock")?.enabled).toBe(true)
  })

  it("leaves official out until it is configured, as the host wire does", async () => {
    const client = createInMemoryKorridClient({ pluginHost: { userAdded: [], repositories: [], installs: [] } })
    const repositories = ok(await client.pluginRepositories())
    expect("official" in repositories).toBe(false)
    expect(repositories).toEqual({ userAdded: [], repositoryInstalls: [] })
  })

  it("lists releases in catalog order with install and trust state", async () => {
    const client = createInMemoryKorridClient()
    const official = ok(await client.pluginCatalog(OFFICIAL)).entries
    expect(official.filter(entry => entry.pluginId === "@korri:ppsspp").map(entry => entry.releaseVersion)).toEqual(["1.18.1"])
    expect(official.filter(entry => entry.pluginId === "@korri:sunshine").map(entry => entry.releaseVersion)).toEqual(["2026.10.1", "2026.09.2"])
    expect(official.find(entry => entry.pluginId === "@korri:tailscale")?.installed).toBe(PluginInstalledState.Disabled)
    const community = ok(await client.pluginCatalog(COMMUNITY)).entries
    expect(community.find(entry => entry.pluginId === "@community:clock")?.publisherTrusted).toBe(false)
    expect(await client.inspectPlugin(COMMUNITY, "@community:clock", "0.3.0")).toMatchObject({ _tag: "Err" })
  })

  it("installs only with the reviewed digest, starts off, then toggles", async () => {
    const client = createInMemoryKorridClient()
    const report = ok(await client.inspectPlugin(OFFICIAL, "@korri:sunshine", "2026.09.2"))
    expect(report.warning).not.toBe("")
    expect(report.ports.allowedTCPPorts.length).toBeGreaterThan(0)
    expect(report.brings).toHaveLength(1)
    expect(await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.09.2", "")).toMatchObject({ _tag: "Err" })
    expect(await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1", report.approval)).toMatchObject({ _tag: "Err" })
    expect(ok(await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.09.2", report.approval))).toEqual({})
    const installed = ok(await client.pluginCatalog(OFFICIAL)).entries.filter(entry => entry.pluginId === "@korri:sunshine")
    expect(installed.map(entry => entry.installed)).toEqual([PluginInstalledState.Disabled, PluginInstalledState.Disabled])
    ok(await client.setPluginEnabled("@korri:sunshine", true))
    expect(ok(await client.pluginCatalog(OFFICIAL)).entries.find(entry => entry.pluginId === "@korri:sunshine")?.installed)
      .toBe(PluginInstalledState.Enabled)
  })

  it("reports a stale approval as needing review", async () => {
    const client = createInMemoryKorridClient()
    expect(await client.setPluginEnabled("@korri:tailscale", true)).toMatchObject({
      _tag: "Err", payload: { code: "PluginNeedsReview" },
    })
    expect(ok(await client.setPluginEnabled("@korri:tailscale", false))).toEqual({})
  })

  it("adds and removes repositories the way the host does", async () => {
    const client = createInMemoryKorridClient()
    expect(await client.addPluginRepository("http://more.example/catalog.json")).toMatchObject({
      _tag: "Err", payload: { code: "PluginRepositoryUrlInvalid" },
    })
    expect(await client.addPluginRepository("https://unreachable.example/catalog.json")).toMatchObject({
      _tag: "Err", payload: { code: "PluginHostRefused" },
    })
    expect(ok(await client.addPluginRepository(OFFICIAL))).toBe(PluginRepositoryAddResult.AlreadyPresent)
    expect(ok(await client.addPluginRepository("https://MORE.example/catalog.json"))).toBe(PluginRepositoryAddResult.Added)
    expect(ok(await client.pluginRepositories()).userAdded).toEqual([COMMUNITY, "https://more.example/catalog.json"])
    expect(await client.removePluginRepository(OFFICIAL)).toMatchObject({ _tag: "Err", payload: { code: "PluginHostRefused" } })
    expect(ok(await client.removePluginRepository(COMMUNITY))).toBe(PluginRepositoryRemoveResult.Removed)
    expect(ok(await client.removePluginRepository(COMMUNITY))).toBe(PluginRepositoryRemoveResult.NotFound)
    expect(await client.pluginCatalog(COMMUNITY)).toMatchObject({ _tag: "Err" })
    // Its install stays listed and keeps working.
    expect(ok(await client.pluginRepositories()).repositoryInstalls.some(item => item.id === "@korri:syncthing")).toBe(true)
    expect(ok(await client.setPluginEnabled("@korri:syncthing", false))).toEqual({})
  })

  it("toggles image-seeded plugins through the same method", async () => {
    const client = createInMemoryKorridClient()
    ok(await client.setPluginEnabled("@korri:mgba", false))
    const settings = ok(await client.settingsSnapshot())
    expect(settings.plugins.find(plugin => plugin.id === "@korri:mgba")?.enabled).toBe(false)
    expect(await client.setPluginEnabled("@korri:absent", true)).toMatchObject({ _tag: "Err" })
  })
})
