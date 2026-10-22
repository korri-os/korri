import { describe, expect, it } from "bun:test"
import type { SurfaceSettingGroup } from "@contracts/surface/korri-surface"
import {
  ADD_REPOSITORY_SETTING,
  OFFICIAL_GROUP,
  configuredRepositories,
  OFFICIAL_REPOSITORY_ROW,
  OTHER_GROUP,
  PLUGINS_GROUP,
  REMOVED_GROUP,
  httpsAddress,
  initialPluginFacts,
  inspectActionId,
  pluginActionFrom,
  pluginChangeFrom,
  pluginSettingGroups,
  releaseRowId,
  removeRepositoryActionId,
  reviewActionId,
  reviewMessage,
  toggleSettingId,
  withCatalog,
  withNeedsReview,
  withRepositories,
  withReview,
  type PluginFacts,
} from "./plugin-repositories"
import {
  BROKEN,
  OFFICIAL,
  REMOVED,
  RETRO,
  err,
  inspection,
  ok,
  readyFacts,
  repositories,
  retroCatalog,
} from "./plugin-repositories.fixtures"

const groupNamed = (groups: readonly SurfaceSettingGroup[], title: string) =>
  groups.find(group => group.title === title)

const chiptune = { sourceUrl: RETRO, pluginId: "@ada:chiptune", releaseVersion: "1.0.0" }

const retroFacts = (over: Partial<PluginFacts> = {}): PluginFacts => readyFacts({
  repositories: { _tag: "Ready", value: repositories({
    userAdded: [RETRO],
    repositoryInstalls: [{ id: "@ada:netplay", sourceUrl: RETRO, releaseVersion: "0.4.0", enabled: true }],
  }) },
  catalogs: { [RETRO]: { _tag: "Ready", entries: retroCatalog } },
  ...over,
})

describe("plugin settings groups", () => {
  it("official only: shows the official repository as not set up, read-only, and the add row", () => {
    const groups = pluginSettingGroups(readyFacts(), [])
    expect(groups.map(group => group.title)).toEqual([PLUGINS_GROUP])
    expect(groups[0]?.items).toEqual([
      {
        id: OFFICIAL_REPOSITORY_ROW,
        label: "Official plugins",
        value: "Not set up yet",
        description: "Korri's own plugins will appear here.",
      },
      {
        id: ADD_REPOSITORY_SETTING,
        label: "Add a repository",
        description: "Type the https:// address the plugin maker gave you.",
        interaction: { kind: "text", placeholder: "https://…", maxLength: 512 },
      },
    ])
  })

  it("lists the official repository first and never offers to remove it", () => {
    const facts = readyFacts({
      repositories: { _tag: "Ready", value: repositories({ official: OFFICIAL, userAdded: [RETRO] }) },
      catalogs: {
        [OFFICIAL]: { _tag: "Ready", entries: [{ pluginId: "@korri:tailscale", title: "Tailscale", releaseVersion: "1.0", publisherTrusted: true }] },
        [RETRO]: { _tag: "Ready", entries: retroCatalog },
      },
    })
    const groups = pluginSettingGroups(facts, [])
    expect(groups.map(group => group.title)).toEqual([PLUGINS_GROUP, OFFICIAL_GROUP, "github.com/ada/retro-plugins"])
    expect(groups[0]?.items[0]).toMatchObject({ id: OFFICIAL_REPOSITORY_ROW, value: "1 plugin", description: "Always here. It can't be removed." })
    expect(groups[0]?.items[0]?.interaction).toBeUndefined()
    const all = groups.flatMap(group => group.items)
    expect(all.some(item => item.id === removeRepositoryActionId(OFFICIAL))).toBe(false)
    expect(all.filter(item => item.interaction?.kind === "action" && item.interaction.destructive).map(item => item.id))
      .toEqual([removeRepositoryActionId(RETRO)])
  })

  it("groups each added repository's plugins, one row per plugin, ending with Remove", () => {
    const retro = groupNamed(pluginSettingGroups(retroFacts(), []), "github.com/ada/retro-plugins")
    expect(retro?.items.map(item => [item.label, item.value ?? null])).toEqual([
      ["Chiptune", "Install"],
      ["CRT look", "Install 2.1.0"],
      ["CRT look", "Install 2.0.3"],
      ["Netplay", "On"],
      ["Free coins", "Not trusted"],
      ["Remove this repository", null],
    ])
    expect(retro?.items.at(-1)?.interaction).toEqual({
      kind: "action",
      actionId: removeRepositoryActionId(RETRO),
      destructive: true,
      confirmation: {
        title: "Remove this repository?",
        message: `Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under "${REMOVED_GROUP}".`,
        confirmLabel: "Remove",
      },
    })
  })

  it("says in the remove question how many installs stay", () => {
    const message = (installs: number) => {
      const facts = retroFacts({ repositories: { _tag: "Ready", value: repositories({
        userAdded: [RETRO],
        repositoryInstalls: Array.from({ length: installs }, (_, index) =>
          ({ id: `@ada:p${index}`, sourceUrl: RETRO, releaseVersion: "1", enabled: true })),
      }) } })
      const remove = groupNamed(pluginSettingGroups(facts, []), "github.com/ada/retro-plugins")?.items.at(-1)?.interaction
      return remove?.kind === "action" ? remove.confirmation?.message : undefined
    }
    expect(message(0)).toBe("Korri stops showing plugins from github.com/ada/retro-plugins. You can add it again later.")
    expect(message(2)).toBe(`Korri stops showing plugins from github.com/ada/retro-plugins. The 2 plugins you installed from it stay, and still turn on and off under "${REMOVED_GROUP}".`)
  })

  it("uses one release silently, and offers several in catalog order without picking one", () => {
    const retro = groupNamed(pluginSettingGroups(retroFacts(), []), "github.com/ada/retro-plugins")
    const chip = retro?.items[0]
    expect(chip).toEqual({
      id: releaseRowId(chiptune),
      label: "Chiptune",
      value: "Install",
      description: "Music player for tracker songs",
      interaction: { kind: "action", actionId: inspectActionId(chiptune) },
    })
    const crt = retro?.items.filter(item => item.label === "CRT look")
    expect(crt?.map(item => item.value)).toEqual(["Install 2.1.0", "Install 2.0.3"])
    expect(crt?.[0]?.description).toBe("Pick the version to install.")
    expect(crt?.map(item => item.interaction)).toEqual([
      { kind: "action", actionId: inspectActionId({ sourceUrl: RETRO, pluginId: "@ada:crt", releaseVersion: "2.1.0" }) },
      { kind: "action", actionId: inspectActionId({ sourceUrl: RETRO, pluginId: "@ada:crt", releaseVersion: "2.0.3" }) },
    ])
  })

  it("shows an untrusted publisher as a read-only row naming the publisher", () => {
    const row = groupNamed(pluginSettingGroups(retroFacts(), []), "github.com/ada/retro-plugins")?.items
      .find(item => item.label === "Free coins")
    expect(row?.description).toBe("Publisher @mallory is not trusted on this device")
    expect(row?.interaction).toBeUndefined()
  })

  it("turns an inspected release into Review and turn on, whose confirmation is the report", () => {
    const facts = withReview(retroFacts(), chiptune, inspection)
    const row = groupNamed(pluginSettingGroups(facts, []), "github.com/ada/retro-plugins")?.items[0]
    expect(row).toEqual({
      id: releaseRowId(chiptune),
      label: "Chiptune",
      value: "Review and turn on",
      interaction: {
        kind: "action",
        actionId: reviewActionId(chiptune),
        confirmation: {
          title: "Turn on Chiptune?",
          message: reviewMessage(inspection),
          confirmLabel: "Turn on",
        },
      },
    })
  })

  it("writes the report in plain words: warning, ports, background programs, and every dependency", () => {
    expect(reviewMessage(inspection).split("\n")).toEqual([
      "Warning: Plays sound over the network to paired speakers.",
      "Opens network ports: TCP 7000, 7001; UDP 5353.",
      "Runs in the background: chiptune-daemon.service.",
      "Also installs Audio core. Warning: Can change the system volume.",
      "Also uses PipeWire bridge. You approved it before.",
    ])
    expect(reviewMessage({ ...inspection, warning: "", ports: { allowedTCPPorts: [], allowedUDPPorts: [] }, nativeUnits: [], brings: [] }))
      .toBe("It asks for no network ports and runs nothing in the background.")
  })

  it("shows an unreachable repository with the host's words while its installs still toggle", () => {
    const facts = retroFacts({ catalogs: { [RETRO]: { _tag: "Unreachable", message: "Could not reach github.com: timed out after 30 s" } } })
    const groups = pluginSettingGroups(facts, [{ id: "@ada:netplay", title: "Netplay", enabled: true }])
    expect(groups[0]?.items[1]).toMatchObject({ label: "github.com/ada/retro-plugins", value: "Can't reach it" })
    const retro = groupNamed(groups, "github.com/ada/retro-plugins")
    expect(retro?.items.map(item => [item.label, item.value ?? null, item.description ?? null])).toEqual([
      ["Can't reach this repository", null, "Could not reach github.com: timed out after 30 s"],
      ["Netplay", "On", null],
      ["Remove this repository", null, null],
    ])
    expect(retro?.items[1]).toMatchObject({ id: toggleSettingId("@ada:netplay"), interaction: { kind: "choice" } })
  })

  it("shows a repository being read as checking", () => {
    const facts = retroFacts({ catalogs: {} })
    const groups = pluginSettingGroups(facts, [])
    expect(groups[0]?.items[1]?.value).toBe("Checking…")
    expect(groupNamed(groups, "github.com/ada/retro-plugins")?.items[0]?.label).toBe("Checking for plugins…")
  })

  it("lists installs from removed repositories with titles from settings, falling back to the id", () => {
    const facts = readyFacts({
      repositories: { _tag: "Ready", value: repositories({
        repositoryInstalls: [
          { id: "@old:shader", sourceUrl: REMOVED, releaseVersion: "1", enabled: false },
          { id: "@old:untitled", sourceUrl: REMOVED, releaseVersion: "1", enabled: true },
        ],
      }) },
    })
    const removed = groupNamed(pluginSettingGroups(facts, [{ id: "@old:shader", title: "Shader pack", enabled: false }]), REMOVED_GROUP)
    expect(removed?.items).toEqual([
      { id: toggleSettingId("@old:shader"), label: "Shader pack", value: "Off", description: "From old.example/plugins", interaction: { kind: "choice", choices: [{ value: "true", label: "On" }, { value: "false", label: "Off" }], selected: "false" } },
      { id: toggleSettingId("@old:untitled"), label: "@old:untitled", value: "On", description: "From old.example/plugins", interaction: { kind: "choice", choices: [{ value: "true", label: "On" }, { value: "false", label: "Off" }], selected: "true" } },
    ])
  })

  it("lists otherInstalls under Other installed plugins, off ones included, never settings.plugins", () => {
    const facts = readyFacts({ repositories: { _tag: "Ready", value: repositories({
      otherInstalls: [
        { id: "@korri:mgba", title: "mGBA", enabled: true },
        { id: "@korri:ssh", enabled: false },
      ],
    }) } })
    // settings.plugins lists only enabled plugins; it must not add or drop rows.
    const groups = pluginSettingGroups(facts, [{ id: "@korri:retroarch", title: "RetroArch", enabled: true }])
    expect(groupNamed(groups, OTHER_GROUP)?.items).toEqual([
      { id: toggleSettingId("@korri:mgba"), label: "mGBA", value: "On", interaction: { kind: "choice", choices: [{ value: "true", label: "On" }, { value: "false", label: "Off" }], selected: "true" } },
      { id: toggleSettingId("@korri:ssh"), label: "@korri:ssh", value: "Off", interaction: { kind: "choice", choices: [{ value: "true", label: "On" }, { value: "false", label: "Off" }], selected: "false" } },
    ])
    expect(groupNamed(pluginSettingGroups(readyFacts(), [{ id: "@korri:retroarch", title: "RetroArch", enabled: true }]), OTHER_GROUP)).toBeUndefined()
  })

  it("shows Needs review on an installed plugin whose approval went stale, and reviews it again", () => {
    const facts = withNeedsReview(retroFacts(), "@ada:netplay", true)
    const row = groupNamed(pluginSettingGroups(facts, []), "github.com/ada/retro-plugins")?.items
      .find(item => item.label === "Netplay")
    expect(row).toMatchObject({
      id: toggleSettingId("@ada:netplay"),
      value: "Needs review",
      interaction: { kind: "action", actionId: inspectActionId({ sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }) },
    })
    const reviewed = withReview(facts, { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }, { ...inspection, id: "@ada:netplay" })
    // A stale installed plugin is reviewed on its own row; it has no release row.
    expect(groupNamed(pluginSettingGroups(reviewed, []), "github.com/ada/retro-plugins")?.items
      .find(item => item.label === "Netplay")).toMatchObject({
      id: toggleSettingId("@ada:netplay"),
      value: "Review and turn on",
      interaction: { kind: "action", actionId: reviewActionId({ sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }) },
    })
  })

  it("treats a missing official repository as not set up", () => {
    expect(pluginSettingGroups(readyFacts(), [])[0]?.items[0]?.value).toBe("Not set up yet")
    expect(configuredRepositories(repositories({ userAdded: [RETRO] }))).toEqual([RETRO])
  })

  it("treats an empty warning as no warning", () => {
    expect(reviewMessage({ ...inspection, warning: "  ", brings: [{ id: "@x:y", title: "Y", warning: "", alreadyApproved: false }] }).split("\n"))
      .toEqual([
        "Opens network ports: TCP 7000, 7001; UDP 5353.",
        "Runs in the background: chiptune-daemon.service.",
        "Also installs Y.",
      ])
  })

  it("a stale approval met on Off asks to review and turn off", () => {
    const netplay = { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }
    const stale = withNeedsReview(retroFacts(), "@ada:netplay", false)
    const row = () => groupNamed(pluginSettingGroups(facts, []), "github.com/ada/retro-plugins")?.items.find(item => item.label === "Netplay")
    let facts = stale
    expect(row()).toMatchObject({ value: "Needs review", description: "It changed since you approved it. Check it again to turn it off." })
    facts = withReview(stale, netplay, { ...inspection, id: "@ada:netplay", title: "Netplay" })
    expect(row()).toMatchObject({
      value: "Review and turn off",
      interaction: { kind: "action", confirmation: { title: "Turn off Netplay?", confirmLabel: "Turn off" } },
    })
  })

  it("says so while repositories load, and shows the host's words when they cannot be read", () => {
    expect(pluginSettingGroups(initialPluginFacts, [])[0]?.items.map(item => [item.id, item.label, item.value])).toEqual([
      [OFFICIAL_REPOSITORY_ROW, "Official plugins", "Checking…"],
      [ADD_REPOSITORY_SETTING, "Add a repository", undefined],
    ])
    const failed = withRepositories(initialPluginFacts, err("BrainUnreachable", "korrid is not answering"))
    const groups = pluginSettingGroups(failed, [{ id: "@korri:mgba", title: "mGBA", enabled: false }])
    expect(groups[0]?.items[0]).toMatchObject({ label: "Official plugins", value: "Can't read the list", description: "korrid is not answering" })
    // The installs arrive with the list, so none show without it.
    expect(groups).toHaveLength(1)
  })

  it("keeps the last repositories when a re-read fails", () => {
    const facts = readyFacts()
    expect(withRepositories(facts, err("BrainUnreachable", "gone"))).toBe(facts)
  })

  it("replaces a catalog with what korrid read, or its failure", () => {
    const facts = withCatalog(readyFacts(), RETRO, ok({ entries: retroCatalog }))
    expect(facts.catalogs[RETRO]).toEqual({ _tag: "Ready", entries: retroCatalog })
    expect(withCatalog(facts, RETRO, err("CatalogInvalid", "The catalog is not valid JSON")).catalogs[RETRO])
      .toEqual({ _tag: "Unreachable", message: "The catalog is not valid JSON" })
  })
})

describe("plugin ids", () => {
  it("round-trips actions whose URLs and ids hold colons and slashes", () => {
    const target = { sourceUrl: "https://x.example:8443/a:b/", pluginId: "@ada:crt", releaseVersion: "2.0.0-rc:1" }
    expect(pluginActionFrom(inspectActionId(target))).toEqual({ _tag: "Inspect", target })
    expect(pluginActionFrom(reviewActionId(target))).toEqual({ _tag: "Review", target })
    expect(pluginActionFrom(removeRepositoryActionId(target.sourceUrl))).toEqual({ _tag: "Remove", url: target.sourceUrl })
    expect(pluginActionFrom("game-folder-rescan")).toBeUndefined()
    expect(pluginActionFrom("plugin-review:%E0%A4%A")).toBeUndefined()
  })

  it("reads toggles and the add row, and nothing else", () => {
    expect(pluginChangeFrom(toggleSettingId("@ada:crt"), "true")).toEqual({ _tag: "Toggle", pluginId: "@ada:crt", enabled: true })
    expect(pluginChangeFrom(toggleSettingId("@ada:crt"), "maybe")).toBeUndefined()
    expect(pluginChangeFrom(ADD_REPOSITORY_SETTING, " https://a.example ")).toEqual({ _tag: "AddRepository", url: " https://a.example " })
    expect(pluginChangeFrom("device-name", "x")).toBeUndefined()
  })

  it("accepts only https addresses with a host", () => {
    expect(httpsAddress("  https://github.com/ada/retro-plugins  ")).toBe("https://github.com/ada/retro-plugins")
    expect(httpsAddress("http://github.com/ada")).toBeUndefined()
    expect(httpsAddress("github.com/ada")).toBeUndefined()
    expect(httpsAddress("https://")).toBeUndefined()
    expect(httpsAddress("ftp://x")).toBeUndefined()
  })

  it("keeps a broken URL from becoming a repository row id collision", () => {
    const facts = readyFacts({ repositories: { _tag: "Ready", value: repositories({ userAdded: [RETRO, BROKEN] }) } })
    const ids = pluginSettingGroups(facts, []).flatMap(group => group.items.map(item => item.id))
    expect(new Set(ids).size).toBe(ids.length)
  })
})
