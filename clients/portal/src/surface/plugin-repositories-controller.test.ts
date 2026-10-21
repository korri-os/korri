import { describe, expect, it } from "bun:test"
import type { SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import {
  ADD_REPOSITORY_SETTING,
  ALREADY_ADDED,
  INVALID_ADDRESS,
  NEEDS_REVIEW_CODE,
  initialPluginFacts,
  inspectActionId,
  releaseRowId,
  removeRepositoryActionId,
  reviewActionId,
  toggleSettingId,
  type PluginFacts,
  type PluginRepositoryClient,
} from "./plugin-repositories"
import { PluginRepositoryAddResult, PluginRepositoryRemoveResult } from "@contracts/generated/korrid"
import { createPluginRepositoriesController } from "./plugin-repositories-controller"
import {
  OFFICIAL,
  RETRO,
  err,
  fakePluginClient,
  inspection,
  ok,
  repositories,
  retroCatalog,
} from "./plugin-repositories.fixtures"

const chiptune = { sourceUrl: RETRO, pluginId: "@ada:chiptune", releaseVersion: "1.0.0" }

function harness(client: PluginRepositoryClient, start: PluginFacts = initialPluginFacts) {
  let facts = start
  let busy = false
  const statuses: SurfaceSettingsStatus[] = []
  let changes = 0
  const controller = createPluginRepositoriesController(client, {
    facts: () => facts,
    publish: next => { facts = next },
    status: next => { statuses.push(next) },
    busy: { get: () => busy, set: value => { busy = value } },
    changed: () => { changes++; void controller.refresh() },
    alive: () => true,
  })
  return {
    controller,
    facts: () => facts,
    statuses,
    last: () => statuses.at(-1),
    busy: () => busy,
    changes: () => changes,
  }
}

const settle = async () => { for (let i = 0; i < 10; i++) await Promise.resolve() }

const baseAnswers = {
  pluginRepositories: () => ok(repositories({ official: OFFICIAL, userAdded: [RETRO] })),
  pluginCatalog: (url: string) => url === OFFICIAL ? ok({ entries: [] }) : ok({ entries: retroCatalog }),
}

describe("plugin repositories controller", () => {
  it("reads repositories, then every configured catalog", async () => {
    const { client, calls } = fakePluginClient(baseAnswers)
    const h = harness(client)
    await h.controller.refresh()
    expect(calls.map(call => call.join(" "))).toEqual(["pluginRepositories", `pluginCatalog ${OFFICIAL}`, `pluginCatalog ${RETRO}`])
    expect(h.facts().catalogs[RETRO]).toEqual({ _tag: "Ready", entries: retroCatalog })
  })

  it("reads catalogs one after another, never two at once", async () => {
    let open = 0
    let most = 0
    const { client } = fakePluginClient({
      pluginRepositories: () => ok(repositories({ official: OFFICIAL, userAdded: [RETRO] })),
      pluginCatalog: async () => {
        most = Math.max(most, ++open)
        await Promise.resolve()
        open--
        return ok({ entries: [] })
      },
    })
    await harness(client).controller.refresh()
    expect(most).toBe(1)
  })

  it("adding: the row saves while korrid checks, then the new repository is read", async () => {
    let answer: (value: ReturnType<typeof ok<PluginRepositoryAddResult>>) => void = () => {}
    const { client, calls } = fakePluginClient({
      ...baseAnswers,
      addPluginRepository: () => new Promise(resolve => { answer = resolve }) as never,
    })
    const h = harness(client)
    expect(h.controller.change(ADD_REPOSITORY_SETTING, "  https://github.com/ada/retro-plugins ")).toBe(true)
    expect(h.last()).toEqual({ _tag: "Saving", settingId: ADD_REPOSITORY_SETTING })
    expect(h.busy()).toBe(true)
    expect(calls).toEqual([["addPluginRepository", "https://github.com/ada/retro-plugins"]])
    answer(ok(PluginRepositoryAddResult.Added))
    await settle()
    expect(h.last()).toEqual({ _tag: "Idle" })
    expect(h.busy()).toBe(false)
    expect(calls.map(call => call[0])).toContain("pluginRepositories")
  })

  it("invalid address: says to use https and asks korrid nothing", () => {
    const { client, calls } = fakePluginClient({})
    const h = harness(client)
    h.controller.change(ADD_REPOSITORY_SETTING, "http://github.com/ada")
    expect(h.last()).toEqual({ _tag: "Problem", settingId: ADD_REPOSITORY_SETTING, message: INVALID_ADDRESS })
    expect(calls).toEqual([])
  })

  it("unreachable or malformed catalog: shows the host's words and reads nothing new", async () => {
    const { client, calls } = fakePluginClient({
      addPluginRepository: () => err("CatalogUnreachable", "Could not reach example.org: connection refused"),
    })
    const h = harness(client)
    h.controller.change(ADD_REPOSITORY_SETTING, "https://example.org/plugins")
    await settle()
    expect(h.last()).toEqual({ _tag: "Problem", settingId: ADD_REPOSITORY_SETTING, message: "Could not reach example.org: connection refused" })
    expect(calls.map(call => call[0])).toEqual(["addPluginRepository"])
    expect(h.busy()).toBe(false)
  })

  it("already added: says so", async () => {
    const { client } = fakePluginClient({ addPluginRepository: () => ok(PluginRepositoryAddResult.AlreadyPresent) })
    const h = harness(client)
    h.controller.change(ADD_REPOSITORY_SETTING, RETRO)
    await settle()
    expect(h.last()).toEqual({ _tag: "Problem", settingId: ADD_REPOSITORY_SETTING, message: ALREADY_ADDED })
  })

  it("remove: runs only the confirmed destructive action, then re-reads", async () => {
    const { client, calls } = fakePluginClient({ ...baseAnswers, removePluginRepository: () => ok(PluginRepositoryRemoveResult.Removed) })
    const h = harness(client)
    expect(h.controller.run(removeRepositoryActionId(RETRO))).toBe(true)
    expect(h.last()).toEqual({ _tag: "Saving", settingId: removeRepositoryActionId(RETRO) })
    await settle()
    expect(calls[0]).toEqual(["removePluginRepository", RETRO])
    expect(h.last()).toEqual({ _tag: "Idle" })
    expect(calls.map(call => call[0])).toContain("pluginRepositories")
  })

  it("install never fires without a confirmed inspection report", async () => {
    const { client, calls } = fakePluginClient({ ...baseAnswers, inspectPlugin: () => ok(inspection) })
    const h = harness(client)
    await h.controller.refresh()
    calls.length = 0
    // Review pressed with nothing inspected: nothing happens.
    expect(h.controller.run(reviewActionId(chiptune))).toBe(true)
    await settle()
    expect(calls).toEqual([])
    // Install inspects and keeps the report; it does not install.
    h.controller.run(inspectActionId(chiptune))
    expect(h.last()).toEqual({ _tag: "Saving", settingId: releaseRowId(chiptune) })
    await settle()
    expect(calls).toEqual([["inspectPlugin", RETRO, "@ada:chiptune", "1.0.0"]])
    expect(h.last()).toEqual({ _tag: "Idle" })
    expect(Object.values(h.facts().reviews).map(review => review.report.approval)).toEqual([inspection.approval])
    // A review for another release is still not approved.
    h.controller.run(reviewActionId({ ...chiptune, releaseVersion: "0.9.0" }))
    await settle()
    expect(calls.map(call => call[0])).not.toContain("installPlugin")
  })

  it("confirm installs with exactly the inspected digest, then turns the plugin on", async () => {
    const { client, calls } = fakePluginClient({
      ...baseAnswers,
      inspectPlugin: () => ok(inspection),
      installPlugin: () => ok({}),
      setPluginEnabled: () => ok({}),
    })
    const h = harness(client)
    await h.controller.refresh()
    h.controller.run(inspectActionId(chiptune))
    await settle()
    calls.length = 0
    h.controller.run(reviewActionId(chiptune))
    expect(h.last()).toEqual({ _tag: "Saving", settingId: releaseRowId(chiptune) })
    await settle()
    expect(calls.slice(0, 2)).toEqual([
      ["installPlugin", RETRO, "@ada:chiptune", "1.0.0", "sha256:4f1c9a7e0d2b"],
      ["setPluginEnabled", "@ada:chiptune", true],
    ])
    expect(h.facts().reviews).toEqual({})
    expect(h.changes()).toBe(1)
    expect(h.last()).toEqual({ _tag: "Idle" })
  })

  it("a refused install shows the host's words and keeps the report for another try", async () => {
    const { client, calls } = fakePluginClient({
      ...baseAnswers,
      inspectPlugin: () => ok(inspection),
      installPlugin: () => err("ApprovalMismatch", "The plugin changed after you reviewed it"),
    })
    const h = harness(client)
    await h.controller.refresh()
    h.controller.run(inspectActionId(chiptune))
    await settle()
    h.controller.run(reviewActionId(chiptune))
    await settle()
    expect(h.last()).toEqual({ _tag: "Problem", settingId: releaseRowId(chiptune), message: "The plugin changed after you reviewed it" })
    expect(calls.map(call => call[0])).not.toContain("setPluginEnabled")
    expect(h.busy()).toBe(false)
  })

  it("later on/off presses call setPluginEnabled directly", async () => {
    const { client, calls } = fakePluginClient({ ...baseAnswers, setPluginEnabled: () => ok({}) })
    const h = harness(client)
    h.controller.change(toggleSettingId("@ada:netplay"), "false")
    expect(h.last()).toEqual({ _tag: "Saving", settingId: toggleSettingId("@ada:netplay") })
    await settle()
    expect(calls[0]).toEqual(["setPluginEnabled", "@ada:netplay", false])
    expect(calls.map(call => call[0])).not.toContain("installPlugin")
    expect(h.changes()).toBe(1)
  })

  it("a stale approval marks the plugin Needs review, and its next press inspects again", async () => {
    const { client, calls } = fakePluginClient({
      pluginRepositories: () => ok(repositories({
        userAdded: [RETRO],
        repositoryInstalls: [{ id: "@ada:netplay", sourceUrl: RETRO, releaseVersion: "0.4.0", enabled: false }],
      })),
      pluginCatalog: () => ok({ entries: retroCatalog }),
      setPluginEnabled: () => err(NEEDS_REVIEW_CODE, "Approval is out of date"),
      inspectPlugin: () => ok({ ...inspection, id: "@ada:netplay" }),
    })
    const h = harness(client)
    await h.controller.refresh()
    h.controller.change(toggleSettingId("@ada:netplay"), "true")
    await settle()
    expect(h.facts().needsReview).toEqual({ "@ada:netplay": true })
    expect(h.last()).toEqual({ _tag: "Idle" })
    h.controller.run(inspectActionId({ sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }))
    expect(h.last()).toEqual({ _tag: "Saving", settingId: toggleSettingId("@ada:netplay") })
    await settle()
    expect(calls.at(-1)).toEqual(["inspectPlugin", RETRO, "@ada:netplay", "0.4.0"])
  })

  it("a stale approval on Off needs review too, and the confirmed review installs, then turns it off", async () => {
    const { client, calls } = fakePluginClient({
      pluginRepositories: () => ok(repositories({
        userAdded: [RETRO],
        repositoryInstalls: [{ id: "@ada:netplay", sourceUrl: RETRO, releaseVersion: "0.4.0", enabled: true }],
      })),
      pluginCatalog: () => ok({ entries: retroCatalog }),
      setPluginEnabled: (_id, enabled) => enabled === false && calls.every(call => call[0] !== "installPlugin")
        ? err(NEEDS_REVIEW_CODE, "Approval is out of date")
        : ok({}),
      inspectPlugin: () => ok({ ...inspection, id: "@ada:netplay" }),
      installPlugin: () => ok({}),
    })
    const h = harness(client)
    await h.controller.refresh()
    h.controller.change(toggleSettingId("@ada:netplay"), "false")
    await settle()
    expect(h.facts().needsReview).toEqual({ "@ada:netplay": false })
    const netplay = { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }
    h.controller.run(inspectActionId(netplay))
    await settle()
    calls.length = 0
    h.controller.run(reviewActionId(netplay))
    await settle()
    expect(calls.slice(0, 2)).toEqual([
      ["installPlugin", RETRO, "@ada:netplay", "0.4.0", inspection.approval],
      ["setPluginEnabled", "@ada:netplay", false],
    ])
    expect(h.facts().needsReview).toEqual({})
    expect(h.last()).toEqual({ _tag: "Idle" })
  })

  it("regression: stale approval, Off, review, confirm leaves the plugin off", async () => {
    // A small stateful host: a re-review keeps the plugin's state (host.rs),
    // and a stale approval refuses any switch until it is re-installed.
    let enabled = true
    let stale = true
    const { client } = fakePluginClient({
      pluginRepositories: () => ok(repositories({
        userAdded: [RETRO],
        repositoryInstalls: [{ id: "@ada:netplay", sourceUrl: RETRO, releaseVersion: "0.4.0", enabled }],
      })),
      pluginCatalog: () => ok({ entries: retroCatalog }),
      inspectPlugin: () => ok({ ...inspection, id: "@ada:netplay" }),
      installPlugin: () => {
        stale = false
        return ok({})
      },
      setPluginEnabled: (_id, next) => {
        if (stale) return err(NEEDS_REVIEW_CODE, "Approval is out of date")
        enabled = next
        return ok({})
      },
    })
    const h = harness(client)
    await h.controller.refresh()
    const netplay = { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }
    h.controller.change(toggleSettingId("@ada:netplay"), "false")
    await settle()
    h.controller.run(inspectActionId(netplay))
    await settle()
    h.controller.run(reviewActionId(netplay))
    await settle()
    await settle()
    expect(enabled).toBe(false)
    expect(h.last()).toEqual({ _tag: "Idle" })
    // The re-read after the change shows it off too.
    const repos = h.facts().repositories
    const install = repos._tag === "Ready"
      ? repos.value.repositoryInstalls.find(candidate => candidate.id === "@ada:netplay")
      : undefined
    expect(install?.enabled).toBe(false)
  })

  it("an install that succeeds but cannot be switched shows why on the plugin's row", async () => {
    const { client } = fakePluginClient({
      ...baseAnswers,
      inspectPlugin: () => ok(inspection),
      installPlugin: () => ok({}),
      setPluginEnabled: () => err("PermissionDenied", "Plugins can only be changed on this device"),
    })
    const h = harness(client)
    await h.controller.refresh()
    h.controller.run(inspectActionId(chiptune))
    await settle()
    h.controller.run(reviewActionId(chiptune))
    await settle()
    expect(h.last()).toEqual({ _tag: "Problem", settingId: toggleSettingId("@ada:chiptune"), message: "Plugins can only be changed on this device" })
    expect(h.facts().reviews).toEqual({})
    expect(h.busy()).toBe(false)
  })

  it("another refusal on a toggle shows the host's words", async () => {
    const { client } = fakePluginClient({ setPluginEnabled: () => err("PermissionDenied", "Plugins can only be changed on this device") })
    const h = harness(client)
    h.controller.change(toggleSettingId("@korri:mgba"), "true")
    await settle()
    expect(h.last()).toEqual({ _tag: "Problem", settingId: toggleSettingId("@korri:mgba"), message: "Plugins can only be changed on this device" })
  })

  it("does one thing at a time, and leaves other settings to the portal", async () => {
    const { client, calls } = fakePluginClient({ addPluginRepository: () => new Promise(() => {}) as never })
    const h = harness(client)
    h.controller.change(ADD_REPOSITORY_SETTING, RETRO)
    h.controller.run(inspectActionId(chiptune))
    h.controller.change(toggleSettingId("@ada:crt"), "true")
    expect(calls.map(call => call[0])).toEqual(["addPluginRepository"])
    expect(h.controller.change("device-name", "usu")).toBe(false)
    expect(h.controller.run("game-folder-rescan")).toBe(false)
  })
})
