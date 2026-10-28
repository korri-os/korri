import { describe, expect, it } from "bun:test"
import { PluginPublisherState } from "@contracts/generated/korrid"
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
  reviewPublisherRowId,
  toggleSettingId,
  PUBLISHERS_GROUP,
  PUBLISHERS_STATUS_ROW,
  keyChangeLines,
  publisherRowId,
  revokeActionId,
  trustActionId,
  withPublishers,
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
  ADA_FINGERPRINT,
  ADA_NEW_FINGERPRINT,
  KORRI_FINGERPRINT,
  LIB_FINGERPRINT,
  MALLORY_FINGERPRINT,
  bringsNewInspection,
  keyChangedInspection,
  lib,
  mallory,
  newPublisherInspection,
  trustedPublishers,
  unsignedInspection,
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
        [OFFICIAL]: { _tag: "Ready", entries: [{ pluginId: "@korri:tailscale", title: "Tailscale", releaseVersion: "1.0", publisher: { namespace: "@korri", state: PluginPublisherState.Bound } }] },
        [RETRO]: { _tag: "Ready", entries: retroCatalog },
      },
    })
    const groups = pluginSettingGroups(facts, [])
    expect(groups.map(group => group.title)).toEqual([PLUGINS_GROUP, OFFICIAL_GROUP, "git.example/ada/retro-plugins"])
    expect(groups[0]?.items[0]).toMatchObject({ id: OFFICIAL_REPOSITORY_ROW, value: "1 plugin", description: "Always here. It can't be removed." })
    expect(groups[0]?.items[0]?.interaction).toBeUndefined()
    const all = groups.flatMap(group => group.items)
    expect(all.some(item => item.id === removeRepositoryActionId(OFFICIAL))).toBe(false)
    expect(all.filter(item => item.interaction?.kind === "action" && item.interaction.destructive).map(item => item.id))
      .toEqual([removeRepositoryActionId(RETRO)])
  })

  it("groups each added repository's plugins, one row per plugin, ending with Remove", () => {
    const retro = groupNamed(pluginSettingGroups(retroFacts(), []), "git.example/ada/retro-plugins")
    expect(retro?.items.map(item => [item.label, item.value ?? null])).toEqual([
      ["Chiptune", "Install"],
      ["CRT look", "Install 2.1.0"],
      ["CRT look", "Install 2.0.3"],
      ["Netplay", "On"],
      ["Free coins", "Install"],
      ["Pocket dither", "Install"],
      ["Remove this repository", null],
    ])
    expect(retro?.items.at(-1)?.interaction).toEqual({
      kind: "action",
      actionId: removeRepositoryActionId(RETRO),
      destructive: true,
      confirmation: {
        title: "Remove this repository?",
        message: `Korri stops showing plugins from git.example/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under "${REMOVED_GROUP}".`,
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
      const remove = groupNamed(pluginSettingGroups(facts, []), "git.example/ada/retro-plugins")?.items.at(-1)?.interaction
      return remove?.kind === "action" ? remove.confirmation?.message : undefined
    }
    expect(message(0)).toBe("Korri stops showing plugins from git.example/ada/retro-plugins. You can add it again later.")
    expect(message(2)).toBe(`Korri stops showing plugins from git.example/ada/retro-plugins. The 2 plugins you installed from it stay, and still turn on and off under "${REMOVED_GROUP}".`)
  })

  it("uses one release silently, and offers several in catalog order without picking one", () => {
    const retro = groupNamed(pluginSettingGroups(retroFacts(), []), "git.example/ada/retro-plugins")
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

  it("offers Install from a publisher not trusted yet, and says so: the key shows after the check", () => {
    const row = groupNamed(pluginSettingGroups(retroFacts(), []), "git.example/ada/retro-plugins")?.items
      .find(item => item.label === "Free coins")
    const miner = { sourceUrl: RETRO, pluginId: "@mallory:miner", releaseVersion: "9.9.9" }
    expect(row).toEqual({
      id: releaseRowId(miner),
      label: "Free coins",
      value: "Install",
      description: "Publisher @mallory is not trusted yet.",
      interaction: { kind: "action", actionId: inspectActionId(miner) },
    })
  })

  it("names the publisher's state on every version of an untrusted plugin's first row only", () => {
    const facts = retroFacts({ catalogs: { [RETRO]: { _tag: "Ready", entries: [
      { pluginId: "@mallory:miner", title: "Free coins", releaseVersion: "9.9.9", publisher: { namespace: "@mallory", state: PluginPublisherState.New } },
      { pluginId: "@mallory:miner", title: "Free coins", releaseVersion: "9.9.8", publisher: { namespace: "@mallory", state: PluginPublisherState.New } },
    ] } } })
    const rows = groupNamed(pluginSettingGroups(facts, []), "git.example/ada/retro-plugins")?.items.filter(item => item.label === "Free coins")
    expect(rows?.map(row => [row.value, row.description ?? null])).toEqual([
      ["Install 9.9.9", "Publisher @mallory is not trusted yet. Pick the version to install."],
      ["Install 9.9.8", null],
    ])
  })

  it("turns an inspected release into Review and turn on, whose confirmation is the report", () => {
    const facts = withReview(retroFacts(), chiptune, inspection)
    const items = groupNamed(pluginSettingGroups(facts, []), "git.example/ada/retro-plugins")?.items ?? []
    const row = items[0]
    // Every publisher the install brings follows it, read-only.
    expect(items.slice(1, 3)).toEqual([
      { id: reviewPublisherRowId(chiptune, "@ada"), label: "Publisher @ada", value: "Trusted", description: `Key ${ADA_FINGERPRINT}. You trusted it on this device.` },
      { id: reviewPublisherRowId(chiptune, "@korri"), label: "Publisher @korri", value: "Configured", description: `Key ${KORRI_FINGERPRINT}. This device's configuration trusts it.` },
    ])
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

  it("writes the report in plain words: publisher, warning, ports, background programs, and every dependency with its publisher", () => {
    expect(reviewMessage(inspection).split("\n")).toEqual([
      `Publisher @ada, trusted on this device. Key ${ADA_FINGERPRINT}.`,
      "Warning: Plays sound over the network to paired speakers.",
      "Opens network ports: TCP 7000, 7001; UDP 5353.",
      "Runs in the background: chiptune-daemon.service.",
      "Also installs Audio core from @ada (trusted). Warning: Can change the system volume.",
      "Also uses PipeWire bridge from @korri (configured). You approved it before.",
    ])
    expect(reviewMessage({ ...inspection, warning: "", ports: { allowedTCPPorts: [], allowedUDPPorts: [] }, nativeUnits: [], brings: [] }).split("\n"))
      .toEqual([`Publisher @ada, trusted on this device. Key ${ADA_FINGERPRINT}.`, "It asks for no network ports and runs nothing in the background."])
  })

  it("shows an unreachable repository with the host's words while its installs still toggle", () => {
    const facts = retroFacts({ catalogs: { [RETRO]: { _tag: "Unreachable", message: "Could not reach git.example: timed out after 30 s" } } })
    const groups = pluginSettingGroups(facts, [{ id: "@ada:netplay", title: "Netplay", enabled: true }])
    expect(groups[0]?.items[1]).toMatchObject({ label: "git.example/ada/retro-plugins", value: "Can't reach it" })
    const retro = groupNamed(groups, "git.example/ada/retro-plugins")
    expect(retro?.items.map(item => [item.label, item.value ?? null, item.description ?? null])).toEqual([
      ["Can't reach this repository", null, "Could not reach git.example: timed out after 30 s"],
      ["Netplay", "On", null],
      ["Remove this repository", null, null],
    ])
    expect(retro?.items[1]).toMatchObject({ id: toggleSettingId("@ada:netplay"), interaction: { kind: "choice" } })
  })

  it("shows a repository being read as checking", () => {
    const facts = retroFacts({ catalogs: {} })
    const groups = pluginSettingGroups(facts, [])
    expect(groups[0]?.items[1]?.value).toBe("Checking…")
    expect(groupNamed(groups, "git.example/ada/retro-plugins")?.items[0]?.label).toBe("Checking for plugins…")
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
    const row = groupNamed(pluginSettingGroups(facts, []), "git.example/ada/retro-plugins")?.items
      .find(item => item.label === "Netplay")
    expect(row).toMatchObject({
      id: toggleSettingId("@ada:netplay"),
      value: "Needs review",
      interaction: { kind: "action", actionId: inspectActionId({ sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }) },
    })
    const reviewed = withReview(facts, { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }, { ...inspection, id: "@ada:netplay" })
    // A stale installed plugin is reviewed on its own row; it has no release row.
    expect(groupNamed(pluginSettingGroups(reviewed, []), "git.example/ada/retro-plugins")?.items
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
    expect(reviewMessage({ ...inspection, warning: "  ", brings: [{ id: "@x:y", title: "Y", warning: "", alreadyApproved: false, publisher: { namespace: "@x", state: PluginPublisherState.Bound } }] }).split("\n"))
      .toEqual([
        `Publisher @ada, trusted on this device. Key ${ADA_FINGERPRINT}.`,
        "Opens network ports: TCP 7000, 7001; UDP 5353.",
        "Runs in the background: chiptune-daemon.service.",
        "Also installs Y from @x (configured).",
      ])
  })

  it("a stale approval met on Off asks to review and turn off", () => {
    const netplay = { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }
    const stale = withNeedsReview(retroFacts(), "@ada:netplay", false)
    const row = () => groupNamed(pluginSettingGroups(facts, []), "git.example/ada/retro-plugins")?.items.find(item => item.label === "Netplay")
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
    expect(pluginActionFrom("library-folder-rescan")).toBeUndefined()
    expect(pluginActionFrom("plugin-review:%E0%A4%A")).toBeUndefined()
  })

  it("reads toggles and the add row, and nothing else", () => {
    expect(pluginChangeFrom(toggleSettingId("@ada:crt"), "true")).toEqual({ _tag: "Toggle", pluginId: "@ada:crt", enabled: true })
    expect(pluginChangeFrom(toggleSettingId("@ada:crt"), "maybe")).toBeUndefined()
    expect(pluginChangeFrom(ADD_REPOSITORY_SETTING, " https://a.example ")).toEqual({ _tag: "AddRepository", url: " https://a.example " })
    expect(pluginChangeFrom("device-name", "x")).toBeUndefined()
  })

  it("accepts only https addresses with a host", () => {
    expect(httpsAddress("  https://git.example/ada/retro-plugins  ")).toBe("https://git.example/ada/retro-plugins")
    expect(httpsAddress("http://git.example/ada")).toBeUndefined()
    expect(httpsAddress("git.example/ada")).toBeUndefined()
    expect(httpsAddress("https://")).toBeUndefined()
    expect(httpsAddress("ftp://x")).toBeUndefined()
  })

  it("keeps a broken URL from becoming a repository row id collision", () => {
    const facts = readyFacts({ repositories: { _tag: "Ready", value: repositories({ userAdded: [RETRO, BROKEN] }) } })
    const ids = pluginSettingGroups(facts, []).flatMap(group => group.items.map(item => item.id))
    expect(new Set(ids).size).toBe(ids.length)
  })
})

describe("publishers on the review", () => {
  const RETRO_GROUP = "git.example/ada/retro-plugins"
  const miner = { sourceUrl: RETRO, pluginId: "@mallory:miner", releaseVersion: "9.9.9" }
  const dither = { sourceUrl: RETRO, pluginId: "@pocket:dither", releaseVersion: "0.1.0" }
  const rowsAfter = (target: typeof chiptune, report: Parameters<typeof withReview>[2]) => {
    const items = groupNamed(pluginSettingGroups(withReview(retroFacts(), target, report), []), RETRO_GROUP)?.items ?? []
    const at = items.findIndex(item => item.id === releaseRowId(target))
    return items.slice(at)
  }
  const confirmationOf = (item: { readonly interaction?: unknown } | undefined) => {
    const interaction = item?.interaction as { kind: string; confirmation?: { title: string; message: string; confirmLabel: string }; destructive?: boolean } | undefined
    return interaction
  }

  it("New: the next press trusts the publisher, after a question that shows its key; Install comes after", () => {
    const [step, publisher] = rowsAfter(miner, newPublisherInspection)
    expect(step).toEqual({
      id: releaseRowId(miner),
      label: "Free coins",
      value: "Trust @mallory",
      description: "@mallory is not trusted yet. Check its key, then trust it.",
      interaction: {
        kind: "action",
        actionId: trustActionId(miner, "@mallory"),
        confirmation: {
          title: "Trust @mallory?",
          message: [
            "Free coins comes from @mallory. This device has not trusted it before.",
            `Key ${MALLORY_FINGERPRINT}.`,
            "Trust it only if the plugin maker gave you this key.",
            "Later plugins from @mallory must have the same key.",
          ].join("\n"),
          confirmLabel: "Trust",
        },
      },
    })
    expect(publisher).toEqual({
      id: reviewPublisherRowId(miner, "@mallory"),
      label: "Publisher @mallory",
      value: "Not trusted yet",
      description: `Key ${MALLORY_FINGERPRINT}. Check it with the plugin maker before you trust it.`,
    })
  })

  it("Key changed: a red question with the key change text, and Trust the new key", () => {
    const [step, publisher] = rowsAfter(chiptune, keyChangedInspection)
    expect(step).toMatchObject({ value: "Trust the new key", description: "The publisher key for @ada changed." })
    const interaction = confirmationOf(step)
    expect(interaction?.destructive).toBe(true)
    expect(interaction?.confirmation?.title).toBe("Trust the new key for @ada?")
    expect(interaction?.confirmation?.confirmLabel).toBe("Trust the new key")
    // Schema sheet row 15, word for word, one sentence per line.
    expect(interaction?.confirmation?.message.split("\n").join(" ")).toBe(
      `The publisher key for @ada changed. Before: ${ADA_FINGERPRINT}. Now: ${ADA_NEW_FINGERPRINT}. This can mean the author replaced their key, or that someone else is publishing as them. Trust the new key only if the author told you about the change.`)
    expect(interaction).toMatchObject({ actionId: trustActionId(chiptune, "@ada") })
    expect(publisher).toMatchObject({ label: "Publisher @ada", value: "Key changed", description: `Before ${ADA_FINGERPRINT}. Now ${ADA_NEW_FINGERPRINT}.` })
    expect(keyChangeLines(keyChangedInspection.publisher)).toHaveLength(5)
  })

  it("Unsigned: the disclosure comes first in the approval, which warns", () => {
    const [step, publisher] = rowsAfter(dither, unsignedInspection)
    expect(step).toMatchObject({ value: "Review and turn on" })
    const interaction = confirmationOf(step)
    expect(interaction?.destructive).toBe(true)
    expect(interaction?.confirmation?.title).toBe("Turn on Pocket dither?")
    expect(interaction?.confirmation?.message.split("\n")).toEqual([
      "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.",
      "Publisher @pocket, not signed.",
      "It asks for no network ports and runs nothing in the background.",
    ])
    expect(publisher).toEqual({
      id: reviewPublisherRowId(dither, "@pocket"),
      label: "Publisher @pocket",
      value: "Not signed",
      description: "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.",
    })
  })

  it("Trusted and Bound: straight to the normal approval, which only asks", () => {
    const [step] = rowsAfter(chiptune, inspection)
    expect(confirmationOf(step)?.destructive).toBeUndefined()
  })

  it("a dependency's new publisher is trusted before the install, and every publisher is listed", () => {
    const rows = rowsAfter(chiptune, bringsNewInspection)
    expect(rows[0]).toMatchObject({ value: "Trust @lib", interaction: { actionId: trustActionId(chiptune, "@lib") } })
    expect(confirmationOf(rows[0])?.confirmation?.message.split("\n")[0]).toBe("Chiptune needs Audio core from @lib. This device has not trusted it before.")
    expect(rows.slice(1, 4).map(row => [row.label, row.value])).toEqual([
      ["Publisher @ada", "Trusted"],
      ["Publisher @lib", "Not trusted yet"],
      ["Publisher @korri", "Configured"],
    ])
    expect(rows[2]?.description).toBe(`Key ${LIB_FINGERPRINT}. Check it with the plugin maker before you trust it.`)
  })

  it("several publishers to trust: one press each, and the question says what follows", () => {
    const both = { ...newPublisherInspection, brings: [{ id: "@lib:audio-core", title: "Audio core", warning: "", alreadyApproved: false, publisher: lib }] }
    const [step] = rowsAfter(miner, both)
    expect(step).toMatchObject({ value: "Trust @mallory" })
    expect(confirmationOf(step)?.confirmation?.message.split("\n").at(-1)).toBe("After this, Free coins also needs you to trust @lib.")
    const [next] = rowsAfter(miner, { ...both, publisher: { ...mallory, state: PluginPublisherState.Trusted } })
    expect(next).toMatchObject({ value: "Trust @lib" })
  })

  it("lists each dependency's publisher and state in the report", () => {
    const report = {
      ...inspection,
      brings: [
        { id: "@lib:a", title: "A", warning: "", alreadyApproved: false, publisher: lib },
        { id: "@ada:b", title: "B", warning: "", alreadyApproved: false, publisher: keyChangedInspection.publisher },
        { id: "@pocket:c", title: "C", warning: "", alreadyApproved: true, publisher: unsignedInspection.publisher },
      ],
    }
    expect(reviewMessage(report).split("\n")).toEqual([
      "C: This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.",
      `Publisher @ada, trusted on this device. Key ${ADA_FINGERPRINT}.`,
      "Warning: Plays sound over the network to paired speakers.",
      "Opens network ports: TCP 7000, 7001; UDP 5353.",
      "Runs in the background: chiptune-daemon.service.",
      "Also installs A from @lib (not trusted yet).",
      "Also installs B from @ada (key changed).",
      "Also uses C from @pocket (not signed). You approved it before.",
    ])
  })

  it("a publisher to trust without a key to send offers to check again, never to trust", () => {
    const { publicKey: _key, ...keyless } = mallory
    const [step] = rowsAfter(miner, { ...newPublisherInspection, publisher: keyless })
    expect(step).toMatchObject({ value: "Check again", interaction: { kind: "action", actionId: inspectActionId(miner) } })
  })

  it("a stale installed plugin whose key changed asks for trust on its own row", () => {
    const netplay = { sourceUrl: RETRO, pluginId: "@ada:netplay", releaseVersion: "0.4.0" }
    const facts = withReview(withNeedsReview(retroFacts(), "@ada:netplay", true), netplay, { ...keyChangedInspection, id: "@ada:netplay", title: "Netplay" })
    const items = groupNamed(pluginSettingGroups(facts, []), RETRO_GROUP)?.items ?? []
    const at = items.findIndex(item => item.id === toggleSettingId("@ada:netplay"))
    expect(items[at]).toMatchObject({ value: "Trust the new key", interaction: { actionId: trustActionId(netplay, "@ada") } })
    expect(items[at + 1]).toMatchObject({ id: reviewPublisherRowId(netplay, "@ada"), value: "Key changed" })
  })
})

describe("the Publishers list", () => {
  const facts = (publishers: PluginFacts["publishers"]) => readyFacts({ publishers })

  it("comes last, lists every trusted publisher with its key and installs, and offers Revoke only for owner-trusted ones", () => {
    const groups = pluginSettingGroups(
      facts({ _tag: "Ready", value: trustedPublishers }),
      [{ id: "@ada:netplay", title: "Netplay", enabled: true }, { id: "@korri:mgba", title: "mGBA", enabled: true }],
    )
    expect(groups.at(-1)?.title).toBe(PUBLISHERS_GROUP)
    const [korri, ada, pixel] = groups.at(-1)?.items ?? []
    expect(korri).toEqual({
      id: publisherRowId("@korri"),
      label: "@korri",
      value: "Configured",
      description: `This device's configuration trusts it. Key ${KORRI_FINGERPRINT}. 2 plugins installed from it.`,
    })
    expect(ada).toEqual({
      id: publisherRowId("@ada"),
      label: "@ada",
      value: "Revoke",
      description: `Trusted on this device. Key ${ADA_FINGERPRINT}. 1 plugin installed from it.`,
      interaction: {
        kind: "action",
        actionId: revokeActionId("@ada"),
        destructive: true,
        confirmation: {
          title: "Revoke @ada?",
          message: [
            "Plugins from @ada stop starting on this device: Netplay.",
            "You can still turn them off and remove them.",
            "To use them again, you check and trust the key of @ada again.",
          ].join("\n"),
          confirmLabel: "Revoke",
        },
      },
    })
    expect(pixel).toMatchObject({ value: "Revoke", description: expect.stringContaining("No plugins installed from it.") })
    const pixelMessage = (pixel?.interaction as { confirmation?: { message: string } } | undefined)?.confirmation?.message
    expect(pixelMessage).toBe([
      "This device stops trusting @pixel-collective-handheld.",
      "To install from @pixel-collective-handheld again, you check and trust its key again.",
    ].join("\n"))
  })

  it("says when nothing is trusted, and shows the host's words when the list cannot be read", () => {
    expect(pluginSettingGroups(facts({ _tag: "Ready", value: { entries: [] } }), []).at(-1)).toEqual({
      title: PUBLISHERS_GROUP,
      items: [{ id: PUBLISHERS_STATUS_ROW, label: "No trusted publishers", description: "Korri asks you to trust a publisher when you install its first plugin." }],
    })
    const failed = withPublishers(readyFacts(), err("BrainUnreachable", "korrid is not answering"))
    expect(pluginSettingGroups(failed, []).at(-1)).toEqual({
      title: PUBLISHERS_GROUP,
      items: [{ id: PUBLISHERS_STATUS_ROW, label: "Can't read the publishers", description: "korrid is not answering" }],
    })
    // Still being read the first time: no group yet.
    expect(pluginSettingGroups(readyFacts(), []).some(group => group.title === PUBLISHERS_GROUP)).toBe(false)
    // Shown even while the repositories cannot be read.
    expect(pluginSettingGroups({ ...initialPluginFacts, publishers: { _tag: "Ready", value: trustedPublishers } }, []).map(group => group.title))
      .toEqual([PLUGINS_GROUP, PUBLISHERS_GROUP])
  })

  it("names an installed plugin by its catalog title, else its id", () => {
    const withTitles = readyFacts({
      publishers: { _tag: "Ready", value: { entries: [{ namespace: "@ada", fingerprint: ADA_FINGERPRINT, state: PluginPublisherState.Trusted, installed: ["@ada:chiptune", "@ada:unknown"] }] } },
      catalogs: { [RETRO]: { _tag: "Ready", entries: retroCatalog } },
    })
    const row = pluginSettingGroups(withTitles, []).at(-1)?.items[0]
    expect((row?.interaction as { confirmation?: { message: string } } | undefined)?.confirmation?.message.split("\n")[0])
      .toBe("Plugins from @ada stop starting on this device: Chiptune, @ada:unknown.")
  })

  it("keeps every row id unique when a publisher shows on a review and in the list", () => {
    const all = withReview(retroFacts({ publishers: { _tag: "Ready", value: trustedPublishers } }), chiptune, bringsNewInspection)
    const ids = pluginSettingGroups(all, []).flatMap(group => group.items.map(item => item.id))
    expect(new Set(ids).size).toBe(ids.length)
  })
})

describe("publisher ids", () => {
  it("round-trips trust and revoke actions whose names hold colons and slashes", () => {
    const target = { sourceUrl: "https://x.example:8443/a:b/", pluginId: "@ada:crt", releaseVersion: "2.0.0-rc:1" }
    expect(pluginActionFrom(trustActionId(target, "@ada:odd"))).toEqual({ _tag: "Trust", target, namespace: "@ada:odd" })
    expect(pluginActionFrom(revokeActionId("@ada"))).toEqual({ _tag: "Revoke", namespace: "@ada" })
    expect(pluginActionFrom("plugin-trust:only-one-part")).toBeUndefined()
    expect(pluginActionFrom("plugin-publisher-revoke:")).toBeUndefined()
  })
})
