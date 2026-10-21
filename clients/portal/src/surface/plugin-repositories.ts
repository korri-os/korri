/**
 * Plugin repositories, as Settings shows them: what each repository offers,
 * what is installed from it, and the two-step approval a first "on" needs.
 *
 * Pure. The controller (`plugin-repositories-controller.ts`) calls korrid and
 * feeds the results in here; this file decides the rows and their words.
 *
 * Wire types are the generated ones (`contracts/generated/korrid.ts`), and
 * the korrid methods are `KorridClient`'s.
 */
import type {
  SurfaceSettingGroup,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"
import {
  PluginInstalledState,
  type PluginCatalogEntry,
  type PluginCatalogOutcome,
  type PluginInspection,
  type PluginRepositories,
  type PluginRepositoriesOutcome,
  type PluginRepositoryInstall,
  type PluginSetting,
} from "@contracts/generated/korrid"
import type { KorridClient } from "../korrid/client"

/** The korrid methods this flow uses. */
export type PluginRepositoryClient = Pick<
  KorridClient,
  | "pluginRepositories"
  | "addPluginRepository"
  | "removePluginRepository"
  | "pluginCatalog"
  | "inspectPlugin"
  | "installPlugin"
  | "setPluginEnabled"
>

/** A warning the host sent. The host sends "" for none. */
const warningText = (warning: string | undefined): string | undefined => {
  const text = warning?.trim()
  return text ? text : undefined
}

/** The host refuses an "on" whose stored approval no longer matches. */
export const NEEDS_REVIEW_CODE = "PluginNeedsReview"

// ---------------------------------------------------------------- state

export type CatalogView =
  | { readonly _tag: "Checking" }
  | { readonly _tag: "Ready"; readonly entries: readonly PluginCatalogEntry[] }
  | { readonly _tag: "Unreachable"; readonly message: string }

/** One release a person may install: where it comes from, and which. */
export interface ReleaseTarget {
  readonly sourceUrl: string
  readonly pluginId: string
  readonly releaseVersion: string
}

export interface PluginReview extends ReleaseTarget {
  readonly report: PluginInspection
}

export interface PluginFacts {
  readonly repositories:
    | { readonly _tag: "Loading" }
    | { readonly _tag: "Failed"; readonly message: string }
    | { readonly _tag: "Ready"; readonly value: PluginRepositories }
  /** By repository URL. */
  readonly catalogs: Readonly<Record<string, CatalogView>>
  /** Inspection reports waiting for the person's yes, by release key. */
  readonly reviews: Readonly<Record<string, PluginReview>>
  /** Installed plugin ids whose stored approval the host called stale. */
  readonly needsReview: readonly string[]
}

export const initialPluginFacts: PluginFacts = {
  repositories: { _tag: "Loading" },
  catalogs: {},
  reviews: {},
  needsReview: [],
}

// ---------------------------------------------------------------- ids

/* Ids are opaque to a surface. URLs and plugin ids hold ":" and "/", so every
 * part is URI-encoded and the parts are joined with ":". */
const enc = encodeURIComponent
const dec = (part: string): string | undefined => {
  try {
    return decodeURIComponent(part)
  } catch {
    return undefined
  }
}

export const ADD_REPOSITORY_SETTING = "plugin-repository-add"
export const OFFICIAL_REPOSITORY_ROW = "plugin-repository:official"
const REPOSITORY_ROW = "plugin-repository:"
const REMOVE_ACTION = "plugin-repository-remove:"
const RELEASE_ROW = "plugin-release:"
const INSPECT_ACTION = "plugin-inspect:"
const REVIEW_ACTION = "plugin-review:"
const TOGGLE_SETTING = "plugin-toggle:"

export const releaseKey = (target: ReleaseTarget): string =>
  [target.sourceUrl, target.pluginId, target.releaseVersion].map(enc).join(":")

const targetOf = (key: string): ReleaseTarget | undefined => {
  const parts = key.split(":").map(dec)
  if (parts.length !== 3 || parts.some(part => part === undefined || part === "")) return undefined
  const [sourceUrl, pluginId, releaseVersion] = parts as [string, string, string]
  return { sourceUrl, pluginId, releaseVersion }
}

export const repositoryRowId = (url: string) => `${REPOSITORY_ROW}${enc(url)}`
export const removeRepositoryActionId = (url: string) => `${REMOVE_ACTION}${enc(url)}`
export const releaseRowId = (target: ReleaseTarget) => `${RELEASE_ROW}${releaseKey(target)}`
export const inspectActionId = (target: ReleaseTarget) => `${INSPECT_ACTION}${releaseKey(target)}`
export const reviewActionId = (target: ReleaseTarget) => `${REVIEW_ACTION}${releaseKey(target)}`
export const toggleSettingId = (pluginId: string) => `${TOGGLE_SETTING}${enc(pluginId)}`

/** What a setting-local plugin action asks for, or undefined when it is not one. */
export type PluginAction =
  | { readonly _tag: "Remove"; readonly url: string }
  | { readonly _tag: "Inspect"; readonly target: ReleaseTarget }
  | { readonly _tag: "Review"; readonly target: ReleaseTarget }

export function pluginActionFrom(actionId: string): PluginAction | undefined {
  if (actionId.startsWith(REMOVE_ACTION)) {
    const url = dec(actionId.slice(REMOVE_ACTION.length))
    return url ? { _tag: "Remove", url } : undefined
  }
  for (const [prefix, tag] of [[INSPECT_ACTION, "Inspect"], [REVIEW_ACTION, "Review"]] as const) {
    if (actionId.startsWith(prefix)) {
      const target = targetOf(actionId.slice(prefix.length))
      return target ? { _tag: tag, target } : undefined
    }
  }
  return undefined
}

/** A plugin setting change, or undefined when `settingId` is not one. */
export type PluginChange =
  | { readonly _tag: "AddRepository"; readonly url: string }
  | { readonly _tag: "Toggle"; readonly pluginId: string; readonly enabled: boolean }

export function pluginChangeFrom(settingId: string, value: string): PluginChange | undefined {
  if (settingId === ADD_REPOSITORY_SETTING) return { _tag: "AddRepository", url: value }
  if (settingId.startsWith(TOGGLE_SETTING)) {
    const pluginId = dec(settingId.slice(TOGGLE_SETTING.length))
    if (!pluginId || (value !== "true" && value !== "false")) return undefined
    return { _tag: "Toggle", pluginId, enabled: value === "true" }
  }
  return undefined
}

/** Whether a setting id names a plugin row, so its change belongs to this flow. */
export const isPluginSettingId = (settingId: string): boolean =>
  settingId === ADD_REPOSITORY_SETTING || settingId.startsWith(TOGGLE_SETTING)

// ---------------------------------------------------------------- reducers

/** The address the person typed, if it is one Korri may fetch. */
export function httpsAddress(typed: string): string | undefined {
  const text = typed.trim()
  if (!/^https:\/\//i.test(text)) return undefined
  try {
    const url = new URL(text)
    return url.protocol === "https:" && url.hostname !== "" ? text : undefined
  } catch {
    return undefined
  }
}

export const INVALID_ADDRESS = "Use an https:// address"
export const ALREADY_ADDED = "Already added"

export const withRepositories = (
  facts: PluginFacts,
  outcome: PluginRepositoriesOutcome,
): PluginFacts => outcome._tag === "Ok"
  ? { ...facts, repositories: { _tag: "Ready", value: outcome.payload } }
  // A failed re-read keeps what was last read; only a first read shows the failure.
  : facts.repositories._tag === "Ready"
    ? facts
    : { ...facts, repositories: { _tag: "Failed", message: outcome.payload.message } }

export const configuredRepositories = (repositories: PluginRepositories): readonly string[] => {
  // `official` is absent until the device names an official catalog.
  const { official } = repositories
  return [...(official === undefined ? [] : [official]), ...repositories.userAdded]
}

/** Mark catalogs being read. A catalog already read keeps its rows meanwhile. */
export const checkingCatalogs = (facts: PluginFacts, urls: readonly string[]): PluginFacts => ({
  ...facts,
  catalogs: Object.fromEntries(urls.map(url => [url, facts.catalogs[url] ?? { _tag: "Checking" as const }])),
})

export const withCatalog = (
  facts: PluginFacts,
  url: string,
  outcome: PluginCatalogOutcome,
): PluginFacts => ({
  ...facts,
  catalogs: {
    ...facts.catalogs,
    [url]: outcome._tag === "Ok"
      ? { _tag: "Ready", entries: outcome.payload.entries }
      : { _tag: "Unreachable", message: outcome.payload.message },
  },
})

export const withoutRepository = (facts: PluginFacts, url: string): PluginFacts => {
  const { [url]: _, ...catalogs } = facts.catalogs
  return {
    ...facts,
    catalogs,
    reviews: Object.fromEntries(Object.entries(facts.reviews).filter(([, review]) => review.sourceUrl !== url)),
  }
}

/** An inspection report waits for the person's yes. */
export const withReview = (facts: PluginFacts, target: ReleaseTarget, report: PluginInspection): PluginFacts => ({
  ...facts,
  reviews: { ...facts.reviews, [releaseKey(target)]: { ...target, report } },
})

/** The report the person said yes to, when one waits for this release. */
export const reviewFor = (facts: PluginFacts, target: ReleaseTarget): PluginReview | undefined =>
  facts.reviews[releaseKey(target)]

/** After an install, or when a review is abandoned. */
export const withoutReview = (facts: PluginFacts, target: ReleaseTarget): PluginFacts => {
  const { [releaseKey(target)]: _, ...reviews } = facts.reviews
  return { ...facts, reviews, needsReview: facts.needsReview.filter(id => id !== target.pluginId) }
}

export const withNeedsReview = (facts: PluginFacts, pluginId: string): PluginFacts =>
  facts.needsReview.includes(pluginId) ? facts : { ...facts, needsReview: [...facts.needsReview, pluginId] }

/** Where an installed plugin came from, so a stale one can be reviewed again. */
export const installOf = (facts: PluginFacts, pluginId: string): PluginRepositoryInstall | undefined =>
  facts.repositories._tag === "Ready"
    ? facts.repositories.value.repositoryInstalls.find(install => install.id === pluginId)
    : undefined

/** The row a release's work is shown on: its installed row, else its release row. */
export const rowIdForRelease = (facts: PluginFacts, target: ReleaseTarget): string => {
  const install = installOf(facts, target.pluginId)
  return install !== undefined && install.sourceUrl === target.sourceUrl
    ? toggleSettingId(target.pluginId)
    : releaseRowId(target)
}

// ---------------------------------------------------------------- words

/** A repository as a person reads it: no scheme, no trailing slash. */
export const repositoryName = (url: string): string =>
  url.replace(/^https:\/\//i, "").replace(/\/+$/, "")

/** "@x" from "@x:name": the namespace a plugin id is published under. */
export const publisherOf = (pluginId: string): string => {
  const colon = pluginId.indexOf(":")
  return colon > 0 ? pluginId.slice(0, colon) : pluginId
}

const count = (n: number, noun: string) => (n === 1 ? `1 ${noun}` : `${n} ${noun}s`)

const listOf = (items: readonly string[]) => items.join(", ")

/**
 * The approval report as plain lines, one fact per line. A surface shows the
 * confirmation message as text; the line breaks keep each fact apart.
 */
export function reviewMessage(report: PluginInspection): string {
  const lines: string[] = []
  const warning = warningText(report.warning)
  if (warning) lines.push(`Warning: ${warning}`)
  const ports = [
    ...(report.ports.allowedTCPPorts.length ? [`TCP ${listOf(report.ports.allowedTCPPorts.map(String))}`] : []),
    ...(report.ports.allowedUDPPorts.length ? [`UDP ${listOf(report.ports.allowedUDPPorts.map(String))}`] : []),
  ]
  if (ports.length) lines.push(`Opens network ports: ${ports.join("; ")}.`)
  if (report.nativeUnits.length) lines.push(`Runs in the background: ${listOf(report.nativeUnits)}.`)
  for (const bring of report.brings) {
    const name = bring.title ?? bring.id
    if (bring.alreadyApproved) {
      lines.push(`Also uses ${name}. You approved it before.`)
    } else {
      const bringWarning = warningText(bring.warning)
      lines.push(`Also installs ${name}.${bringWarning ? ` Warning: ${bringWarning}` : ""}`)
    }
  }
  if (lines.length === 0) lines.push("It asks for no network ports and runs nothing in the background.")
  return lines.join("\n")
}

// ---------------------------------------------------------------- rows

const onOff = [
  { value: "true", label: "On" },
  { value: "false", label: "Off" },
] as const

/** The second step: the kept report, shown before anything installs. */
const reviewRow = (
  id: string,
  label: string,
  review: PluginReview,
  description: string | undefined,
): SurfaceSettingItem => ({
  id,
  label,
  value: "Review and turn on",
  ...(description === undefined ? {} : { description }),
  interaction: {
    kind: "action",
    actionId: reviewActionId(review),
    confirmation: {
      title: `Turn on ${review.report.title ?? label}?`,
      message: reviewMessage(review.report),
      confirmLabel: "Turn on",
    },
  },
})

const toggleRow = (
  pluginId: string,
  label: string,
  enabled: boolean,
  facts: PluginFacts,
  description: string | undefined,
): SurfaceSettingItem => {
  const install = installOf(facts, pluginId)
  if (facts.needsReview.includes(pluginId) && install !== undefined) {
    const target = { sourceUrl: install.sourceUrl, pluginId, releaseVersion: install.releaseVersion }
    const review = reviewFor(facts, target)
    if (review !== undefined) return reviewRow(toggleSettingId(pluginId), label, review, description)
    return {
      id: toggleSettingId(pluginId),
      label,
      value: "Needs review",
      description: "It changed since you approved it. Check it again to turn it on.",
      interaction: { kind: "action", actionId: inspectActionId(target) },
    }
  }
  return {
    id: toggleSettingId(pluginId),
    label,
    value: enabled ? "On" : "Off",
    ...(description === undefined ? {} : { description }),
    interaction: { kind: "choice", choices: onOff },
  }
}

const releaseRow = (
  target: ReleaseTarget,
  label: string,
  facts: PluginFacts,
  several: boolean,
  description: string | undefined,
): SurfaceSettingItem => {
  const review = reviewFor(facts, target)
  if (review !== undefined) {
    return reviewRow(releaseRowId(target), label, review, several ? `Version ${target.releaseVersion}` : undefined)
  }
  return {
    id: releaseRowId(target),
    label,
    value: several ? `Install ${target.releaseVersion}` : "Install",
    ...(description === undefined ? {} : { description }),
    interaction: { kind: "action", actionId: inspectActionId(target) },
  }
}

/** Every row one repository's catalog yields, plugins in catalog order. */
function catalogRows(
  url: string,
  catalog: CatalogView | undefined,
  installs: readonly PluginRepositoryInstall[],
  titles: ReadonlyMap<string, string>,
  facts: PluginFacts,
): SurfaceSettingItem[] {
  const fromHere = installs.filter(install => install.sourceUrl === url)
  const installedRows = () => fromHere.map(install =>
    toggleRow(install.id, titles.get(install.id) ?? install.id, install.enabled, facts, undefined))
  if (catalog === undefined || catalog._tag === "Checking") {
    return [
      { id: `${repositoryRowId(url)}:status`, label: "Checking for plugins…" },
      ...installedRows(),
    ]
  }
  if (catalog._tag === "Unreachable") {
    return [
      {
        id: `${repositoryRowId(url)}:status`,
        label: "Can't reach this repository",
        description: catalog.message,
      },
      ...installedRows(),
    ]
  }
  const order: string[] = []
  const releases = new Map<string, PluginCatalogEntry[]>()
  for (const entry of catalog.entries) {
    const known = releases.get(entry.pluginId)
    if (known) known.push(entry)
    else {
      order.push(entry.pluginId)
      releases.set(entry.pluginId, [entry])
    }
  }
  const rows: SurfaceSettingItem[] = []
  for (const pluginId of order) {
    const entries = releases.get(pluginId) ?? []
    const first = entries[0]
    if (first === undefined) continue
    const label = first.title ?? titles.get(pluginId) ?? pluginId
    const install = fromHere.find(candidate => candidate.id === pluginId)
    const installedEntry = entries.find(entry => entry.installed !== undefined)
    if (install !== undefined || installedEntry !== undefined) {
      rows.push(toggleRow(
        pluginId,
        label,
        install?.enabled ?? installedEntry?.installed === PluginInstalledState.Enabled,
        facts,
        first.description,
      ))
      continue
    }
    if (!first.publisherTrusted) {
      rows.push({
        id: releaseRowId({ sourceUrl: url, pluginId, releaseVersion: first.releaseVersion }),
        label,
        value: "Not trusted",
        description: `Publisher ${publisherOf(pluginId)} is not trusted on this device`,
      })
      continue
    }
    const several = entries.length > 1
    entries.forEach((entry, index) => {
      rows.push(releaseRow(
        { sourceUrl: url, pluginId, releaseVersion: entry.releaseVersion },
        label,
        facts,
        several,
        index === 0
          ? several ? "Pick the version to install." : first.description
          : undefined,
      ))
    })
  }
  // An install the catalog no longer lists still turns on and off.
  for (const install of fromHere) {
    if (!releases.has(install.id)) {
      rows.push(toggleRow(install.id, titles.get(install.id) ?? install.id, install.enabled, facts, undefined))
    }
  }
  if (rows.length === 0) rows.push({ id: `${repositoryRowId(url)}:status`, label: "No plugins for this device" })
  return rows
}

const catalogStatus = (catalog: CatalogView | undefined): string => {
  if (catalog === undefined || catalog._tag === "Checking") return "Checking…"
  if (catalog._tag === "Unreachable") return "Can't reach it"
  return count(new Set(catalog.entries.map(entry => entry.pluginId)).size, "plugin")
}

export const PLUGINS_GROUP = "Plugins"
export const OFFICIAL_GROUP = "Official plugins"
export const REMOVED_GROUP = "Installed from removed repositories"
export const OTHER_GROUP = "Other installed plugins"

/**
 * Settings groups for plugins, in this order:
 *
 *   Plugins                 the official repository, each added one, and the
 *                           row that adds another
 *   Official plugins        what the official repository offers
 *   <repository>            what one added repository offers, then Remove
 *   Installed from removed repositories
 *   Other installed plugins installed some other way, such as in the image
 *
 * `installed` is `SettingsSnapshot.plugins`: titles for installs, and the
 * plugins no repository installed.
 */
export function pluginSettingGroups(
  facts: PluginFacts,
  installed: readonly PluginSetting[] | undefined,
): SurfaceSettingGroup[] {
  const titles = new Map((installed ?? []).map(plugin => [plugin.id, plugin.title]))
  const repositories = facts.repositories
  if (repositories._tag !== "Ready") {
    const hub: SurfaceSettingGroup = {
      title: PLUGINS_GROUP,
      items: [{
        id: OFFICIAL_REPOSITORY_ROW,
        label: "Plugin repositories",
        value: repositories._tag === "Loading" ? "Checking…" : "Can't read them",
        ...(repositories._tag === "Failed" ? { description: repositories.message } : {}),
      }],
    }
    // Without the install list, every installed plugin is shown as other.
    const other = (installed ?? []).map(plugin =>
      toggleRow(plugin.id, plugin.title, plugin.enabled, facts, undefined))
    return other.length ? [hub, { title: OTHER_GROUP, items: other }] : [hub]
  }
  const { userAdded, repositoryInstalls } = repositories.value
  const { official } = repositories.value
  const hub: SurfaceSettingGroup = {
    title: PLUGINS_GROUP,
    items: [
      official === undefined
        ? {
            id: OFFICIAL_REPOSITORY_ROW,
            label: "Official plugins",
            value: "Not set up yet",
            description: "Korri's own plugins will appear here.",
          }
        : {
            id: OFFICIAL_REPOSITORY_ROW,
            label: "Official plugins",
            value: catalogStatus(facts.catalogs[official]),
            description: "Always here. It can't be removed.",
          },
      ...userAdded.map(url => ({
        id: repositoryRowId(url),
        label: repositoryName(url),
        value: catalogStatus(facts.catalogs[url]),
      })),
      {
        id: ADD_REPOSITORY_SETTING,
        label: "Add a repository",
        description: "Type the https:// address the plugin maker gave you.",
        interaction: { kind: "text", placeholder: "https://…", maxLength: 512 },
      },
    ],
  }
  const groups: SurfaceSettingGroup[] = [hub]
  if (official !== undefined) {
    groups.push({
      title: OFFICIAL_GROUP,
      items: catalogRows(official, facts.catalogs[official], repositoryInstalls, titles, facts),
    })
  }
  for (const url of userAdded) {
    const fromHere = repositoryInstalls.filter(install => install.sourceUrl === url).length
    groups.push({
      title: repositoryName(url),
      items: [
        ...catalogRows(url, facts.catalogs[url], repositoryInstalls, titles, facts),
        {
          id: removeRepositoryActionId(url),
          label: "Remove this repository",
          interaction: {
            kind: "action",
            actionId: removeRepositoryActionId(url),
            destructive: true,
            confirmation: {
              title: "Remove this repository?",
              message: fromHere === 0
                ? `Korri stops showing plugins from ${repositoryName(url)}. You can add it again later.`
                : fromHere === 1
                  ? `Korri stops showing plugins from ${repositoryName(url)}. The plugin you installed from it stays, and still turns on and off under "${REMOVED_GROUP}".`
                  : `Korri stops showing plugins from ${repositoryName(url)}. The ${fromHere} plugins you installed from it stay, and still turn on and off under "${REMOVED_GROUP}".`,
              confirmLabel: "Remove",
            },
          },
        },
      ],
    })
  }
  const configured = new Set(configuredRepositories(repositories.value))
  const removed = repositoryInstalls.filter(install => !configured.has(install.sourceUrl))
  if (removed.length) {
    groups.push({
      title: REMOVED_GROUP,
      items: removed.map(install => toggleRow(
        install.id,
        titles.get(install.id) ?? install.id,
        install.enabled,
        facts,
        `From ${repositoryName(install.sourceUrl)}`,
      )),
    })
  }
  const fromRepositories = new Set(repositoryInstalls.map(install => install.id))
  const other = (installed ?? []).filter(plugin => !fromRepositories.has(plugin.id))
  if (other.length) {
    groups.push({
      title: OTHER_GROUP,
      items: other.map(plugin => toggleRow(plugin.id, plugin.title, plugin.enabled, facts, undefined)),
    })
  }
  return groups
}
