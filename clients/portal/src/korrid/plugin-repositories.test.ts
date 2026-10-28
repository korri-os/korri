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
      await client.pluginPublishers()
      await client.trustPublisher("@ada", "ada-plugins-1:QWRhIHNhbXBsZSBrZXksIG5vdCBhIHJlYWwga2V5IQ==")
      await client.revokePublisher("@ada")
      expect(bodies).toEqual([
        { _tag: "system.plugins.repositories", payload: {} },
        { _tag: "system.plugins.repository.add", payload: { url: COMMUNITY } },
        { _tag: "system.plugins.repository.remove", payload: { url: COMMUNITY } },
        { _tag: "system.plugins.catalog", payload: { url: OFFICIAL } },
        { _tag: "system.plugins.inspect", payload: { url: OFFICIAL, pluginId: "@korri:sunshine", releaseVersion: "2026.10.1" } },
        { _tag: "system.plugins.install", payload: { url: OFFICIAL, pluginId: "@korri:sunshine", releaseVersion: "2026.10.1", approval: "digest" } },
        { _tag: "system.plugins.setEnabled", payload: { id: "@korri:sunshine", enabled: true } },
        { _tag: "system.plugins.publishers", payload: {} },
        { _tag: "system.plugins.publisher.trust", payload: { namespace: "@ada", publicKey: "ada-plugins-1:QWRhIHNhbXBsZSBrZXksIG5vdCBhIHJlYWwga2V5IQ==" } },
        { _tag: "system.plugins.publisher.revoke", payload: { namespace: "@ada" } },
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
      expect(await client.trustPublisher("@ada", "k:AA==")).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
      expect(await client.pluginPublishers()).toMatchObject({ _tag: "Err", payload: { code: "PermissionDenied" } })
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
    expect(repositories).toEqual({ userAdded: [], repositoryInstalls: [], otherInstalls: [] })
  })

  it("lists releases in catalog order with install and trust state", async () => {
    const client = createInMemoryKorridClient()
    const official = ok(await client.pluginCatalog(OFFICIAL)).entries
    expect(official.filter(entry => entry.pluginId === "@korri:ppsspp").map(entry => entry.releaseVersion)).toEqual(["1.18.1"])
    expect(official.filter(entry => entry.pluginId === "@korri:sunshine").map(entry => entry.releaseVersion)).toEqual(["2026.10.1", "2026.09.2"])
    expect(official.find(entry => entry.pluginId === "@korri:tailscale")?.installed).toBe(PluginInstalledState.Disabled)
    expect(official.every(entry => entry.publisher.state === "Bound")).toBe(true)
    const community = ok(await client.pluginCatalog(COMMUNITY)).entries
    // A catalog states Bound or Trusted, else New, and never a key.
    expect(Object.fromEntries(community.map(entry => [entry.pluginId, entry.publisher]))).toEqual({
      "@korri:syncthing": { namespace: "@korri", state: "Bound" },
      "@ada:chiptune": { namespace: "@ada", state: "Trusted" },
      "@community:clock": { namespace: "@community", state: "New" },
      "@pixel:scanlines": { namespace: "@pixel", state: "New" },
      "@homebrew:dither": { namespace: "@homebrew", state: "New" },
    })
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

  it("reports a stale approval as needing review, and a new review clears it", async () => {
    const client = createInMemoryKorridClient()
    expect(await client.setPluginEnabled("@korri:tailscale", true)).toMatchObject({
      _tag: "Err", payload: { code: "PluginNeedsReview" },
    })
    expect(ok(await client.setPluginEnabled("@korri:tailscale", false))).toEqual({})
    const report = ok(await client.inspectPlugin(OFFICIAL, "@korri:tailscale", "1.84.0"))
    expect(ok(await client.installPlugin(OFFICIAL, "@korri:tailscale", "1.84.0", report.approval))).toEqual({})
    expect(ok(await client.setPluginEnabled("@korri:tailscale", true))).toEqual({})
    // Another release or source for an installed plugin is not a review.
    expect(await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1",
      ok(await client.inspectPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1")).approval)).toMatchObject({ _tag: "Ok" })
    expect(await client.installPlugin(OFFICIAL, "@korri:sunshine", "2026.09.2",
      ok(await client.inspectPlugin(OFFICIAL, "@korri:sunshine", "2026.09.2")).approval)).toMatchObject({
      _tag: "Err", payload: { message: "plugin is installed; use update or explicit source switch" },
    })
  })

  it("lists other installs on and off, and keeps a turned-off one listed", async () => {
    const client = createInMemoryKorridClient()
    const others = ok(await client.pluginRepositories()).otherInstalls
    expect(others.filter(item => item.enabled).length).toBeGreaterThan(0)
    expect(others.find(item => item.id === "@korri:ssh")).toEqual({ id: "@korri:ssh", title: "SSH", enabled: false })
    ok(await client.setPluginEnabled("@korri:mgba", false))
    expect(ok(await client.pluginRepositories()).otherInstalls.find(item => item.id === "@korri:mgba")?.enabled).toBe(false)
    ok(await client.setPluginEnabled("@korri:ssh", true))
    expect(ok(await client.pluginRepositories()).otherInstalls.find(item => item.id === "@korri:ssh")?.enabled).toBe(true)
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

describe("publisher trust in the in-memory brain", () => {
  const states = async (client: ReturnType<typeof createInMemoryKorridClient>, pluginId: string, version: string) => {
    const report = ok(await client.inspectPlugin(COMMUNITY, pluginId, version))
    return [report.publisher, ...report.brings.map(bring => bring.publisher)].map(publisher => publisher.state)
  }

  it("reaches every publisher state on inspection", async () => {
    const client = createInMemoryKorridClient()
    expect((ok(await client.inspectPlugin(OFFICIAL, "@korri:sunshine", "2026.10.1"))).publisher).toEqual({
      namespace: "@korri", fingerprint: "korri-plugins-1 S29y cmkg c2Ft cGxl", state: "Bound",
    })
    expect(await states(client, "@ada:chiptune", "1.0.0")).toEqual(["Trusted", "New", "Bound"])
    expect(await states(client, "@community:clock", "0.3.0")).toEqual(["New"])
    expect(await states(client, "@pixel:scanlines", "2.0.0")).toEqual(["KeyChanged"])
    expect(await states(client, "@homebrew:dither", "0.1.0")).toEqual(["Unsigned"])
    const changed = ok(await client.inspectPlugin(COMMUNITY, "@pixel:scanlines", "2.0.0")).publisher
    expect(changed.previousFingerprint).not.toBe(changed.fingerprint)
    expect(changed.publicKey).toStartWith("pixel-2:")
    const unsigned = ok(await client.inspectPlugin(COMMUNITY, "@homebrew:dither", "0.1.0"))
    expect(unsigned.publisher.disclosure).toStartWith("This plugin is not signed.")
    expect(unsigned.approval).toEndWith(":unsigned")
  })

  it("refuses an install until every publisher it brings is trusted, then installs", async () => {
    const client = createInMemoryKorridClient()
    const report = ok(await client.inspectPlugin(COMMUNITY, "@ada:chiptune", "1.0.0"))
    expect(await client.installPlugin(COMMUNITY, "@ada:chiptune", "1.0.0", report.approval)).toMatchObject({
      _tag: "Err", payload: { code: "PluginPublisherNotTrusted", message: "publisher @lib is not trusted on this device" },
    })
    const lib = report.brings.find(bring => bring.publisher.namespace === "@lib")?.publisher
    ok(await client.trustPublisher("@lib", lib?.publicKey ?? ""))
    const again = ok(await client.inspectPlugin(COMMUNITY, "@ada:chiptune", "1.0.0"))
    expect(again.brings.map(bring => bring.publisher.state)).toEqual(["Trusted", "Bound"])
    expect(ok(await client.installPlugin(COMMUNITY, "@ada:chiptune", "1.0.0", again.approval))).toEqual({})
  })

  it("refuses a changed key with the old and new fingerprints until the new key is trusted", async () => {
    const client = createInMemoryKorridClient()
    const report = ok(await client.inspectPlugin(COMMUNITY, "@pixel:scanlines", "2.0.0"))
    const refused = await client.installPlugin(COMMUNITY, "@pixel:scanlines", "2.0.0", report.approval)
    expect(refused).toMatchObject({ _tag: "Err", payload: {
      code: "PluginPublisherKeyChanged",
      message: `publisher @pixel key changed: was ${report.publisher.previousFingerprint}, now ${report.publisher.fingerprint}`,
    } })
    ok(await client.trustPublisher("@pixel", report.publisher.publicKey ?? ""))
    expect(ok(await client.inspectPlugin(COMMUNITY, "@pixel:scanlines", "2.0.0")).publisher.state).toBe("Trusted")
  })

  it("lists bound and trusted publishers with what is installed from each, and revokes a trusted one", async () => {
    const client = createInMemoryKorridClient()
    const listed = ok(await client.pluginPublishers()).entries
    expect(listed.map(entry => [entry.namespace, entry.state])).toEqual([["@korri", "Bound"], ["@ada", "Trusted"], ["@pixel", "Trusted"]])
    expect(listed.find(entry => entry.namespace === "@pixel")?.installed).toEqual(["@pixel:palette"])
    expect(listed.find(entry => entry.namespace === "@korri")?.installed).toContain("@korri:mgba")
    expect(await client.revokePublisher("@korri")).toMatchObject({ _tag: "Err" })
    ok(await client.revokePublisher("@pixel"))
    expect(ok(await client.pluginPublishers()).entries.map(entry => entry.namespace)).toEqual(["@korri", "@ada"])
    // Its plugins no longer start; off still works.
    expect(await client.setPluginEnabled("@pixel:palette", true)).toMatchObject({ _tag: "Err", payload: { code: "PluginPublisherNotTrusted" } })
    expect(ok(await client.setPluginEnabled("@pixel:palette", false))).toEqual({})
  })

  it("keeps the unsigned namespace rule: no trust over unsigned installs, no unsigned over trust", async () => {
    const client = createInMemoryKorridClient()
    const report = ok(await client.inspectPlugin(COMMUNITY, "@homebrew:dither", "0.1.0"))
    ok(await client.installPlugin(COMMUNITY, "@homebrew:dither", "0.1.0", report.approval))
    expect(await client.trustPublisher("@homebrew", "homebrew-1:SG9tZWJyZXcgc2FtcGxlIGtleSE=")).toMatchObject({ _tag: "Err", payload: {
      code: "PluginNamespaceTaken", message: "remove the unsigned @homebrew plugins first: @homebrew:dither",
    } })
    expect(await client.trustPublisher("@korri", "korri-plugins-2:T3RoZXI=")).toMatchObject({ _tag: "Err", payload: { code: "PluginNamespaceTaken" } })
    const pinned = createInMemoryKorridClient({ pluginHost: {
      userAdded: [COMMUNITY], installs: [], pins: { "@homebrew": "homebrew-1:SG9tZWJyZXcgc2FtcGxlIGtleSE=" },
      repositories: [{ url: COMMUNITY, entries: [{ pluginId: "@homebrew:dither", releaseVersion: "0.1.0" }] }],
    } })
    expect(await pinned.inspectPlugin(COMMUNITY, "@homebrew:dither", "0.1.0")).toMatchObject({ _tag: "Err", payload: { code: "PluginNamespaceTaken" } })
  })
})
