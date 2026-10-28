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
  type PluginRepositories,
  type PluginRepositoriesOutcome,
  type PluginRepositoryInstall,
  type PluginSetting,
} from "@contracts/generated/korrid"
import type { KorridClient } from "../korrid/client"
import {
  UNSIGNED_DISCLOSURE,
  type PluginCatalogEntry,
  type PluginCatalogOutcome,
  type PluginInspection,
  type PluginPublisher,
  type PluginPublisherEntry,
  type PluginPublishers,
  type PluginPublishersOutcome,
  type PluginPublisherState,
} from "../korrid/plugin-admin"

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
  | "pluginPublishers"
  | "trustPublisher"
  | "revokePublisher"
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
  /** Installed plugins whose stored approval the host called stale, by id,
   * with the state the person asked for when it refused (on or off). */
  readonly needsReview: Readonly<Record<string, boolean>>
  /** The publishers this device trusts (bound or pinned). */
  readonly publishers:
    | { readonly _tag: "Loading" }
    | { readonly _tag: "Failed"; readonly message: string }
    | { readonly _tag: "Ready"; readonly value: PluginPublishers }
}

export const initialPluginFacts: PluginFacts = {
  repositories: { _tag: "Loading" },
  catalogs: {},
  reviews: {},
  needsReview: {},
  publishers: { _tag: "Loading" },
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
const TRUST_ACTION = "plugin-trust:"
const REVIEW_PUBLISHER_ROW = "plugin-review-publisher:"
const PUBLISHER_ROW = "plugin-publisher:"
const REVOKE_ACTION = "plugin-publisher-revoke:"
export const PUBLISHERS_STATUS_ROW = "plugin-publishers:status"

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
/** Trust `namespace`'s key as the kept review of `target` reported it. */
export const trustActionId = (target: ReleaseTarget, namespace: string) =>
  `${TRUST_ACTION}${releaseKey(target)}:${enc(namespace)}`
/** A publisher an inspected release brings, shown under its review. */
export const reviewPublisherRowId = (target: ReleaseTarget, namespace: string) =>
  `${REVIEW_PUBLISHER_ROW}${releaseKey(target)}:${enc(namespace)}`
/** A publisher in the Publishers list. */
export const publisherRowId = (namespace: string) => `${PUBLISHER_ROW}${enc(namespace)}`
export const revokeActionId = (namespace: string) => `${REVOKE_ACTION}${enc(namespace)}`

/** What a setting-local plugin action asks for, or undefined when it is not one. */
export type PluginAction =
  | { readonly _tag: "Remove"; readonly url: string }
  | { readonly _tag: "Inspect"; readonly target: ReleaseTarget }
  | { readonly _tag: "Review"; readonly target: ReleaseTarget }
  | { readonly _tag: "Trust"; readonly target: ReleaseTarget; readonly namespace: string }
  | { readonly _tag: "Revoke"; readonly namespace: string }

export function pluginActionFrom(actionId: string): PluginAction | undefined {
  if (actionId.startsWith(REMOVE_ACTION)) {
    const url = dec(actionId.slice(REMOVE_ACTION.length))
    return url ? { _tag: "Remove", url } : undefined
  }
  if (actionId.startsWith(REVOKE_ACTION)) {
    const namespace = dec(actionId.slice(REVOKE_ACTION.length))
    return namespace ? { _tag: "Revoke", namespace } : undefined
  }
  if (actionId.startsWith(TRUST_ACTION)) {
    const key = actionId.slice(TRUST_ACTION.length)
    const split = key.lastIndexOf(":")
    const target = split > 0 ? targetOf(key.slice(0, split)) : undefined
    const namespace = split > 0 ? dec(key.slice(split + 1)) : undefined
    return target && namespace ? { _tag: "Trust", target, namespace } : undefined
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

export const withPublishers = (
  facts: PluginFacts,
  outcome: PluginPublishersOutcome,
): PluginFacts => outcome._tag === "Ok"
  ? { ...facts, publishers: { _tag: "Ready", value: outcome.payload } }
  // As for repositories: a failed re-read keeps what was last read.
  : facts.publishers._tag === "Ready"
    ? facts
    : { ...facts, publishers: { _tag: "Failed", message: outcome.payload.message } }

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

/** The kept report is no longer true (a trust changed it, and the check
 * after it failed). A stale installed plugin still needs review. */
export const withoutReport = (facts: PluginFacts, target: ReleaseTarget): PluginFacts => {
  const { [releaseKey(target)]: _, ...reviews } = facts.reviews
  return { ...facts, reviews }
}

/** After an install, or when a review is abandoned. */
export const withoutReview = (facts: PluginFacts, target: ReleaseTarget): PluginFacts => {
  const { [releaseKey(target)]: _, ...reviews } = facts.reviews
  const { [target.pluginId]: __, ...needsReview } = facts.needsReview
  return { ...facts, reviews, needsReview }
}

/** The host refused to turn `pluginId` to `wanted` until it is reviewed again. */
export const withNeedsReview = (facts: PluginFacts, pluginId: string, wanted: boolean): PluginFacts =>
  ({ ...facts, needsReview: { ...facts.needsReview, [pluginId]: wanted } })

/** What a confirmed review ends in: on for a first install, else what the
 * person asked for when the host called the approval stale. */
export const wantedAfterReview = (facts: PluginFacts, pluginId: string): boolean =>
  facts.needsReview[pluginId] ?? true

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

// ---------------------------------------------------------------- publishers

/** Bound or Trusted: the host installs from it without asking again. */
export const isTrustedPublisher = (publisher: PluginPublisher): boolean =>
  publisher.state === "Bound" || publisher.state === "Trusted"

/** The owner must trust this key before anything from it installs. */
const awaitsTrust = (publisher: PluginPublisher): boolean =>
  publisher.state === "New" || publisher.state === "KeyChanged"

/** A publisher's state as the value of its row. */
export const PUBLISHER_STATE_LABEL: Readonly<Record<PluginPublisherState, string>> = {
  Bound: "Configured",
  Trusted: "Trusted",
  New: "Not trusted yet",
  KeyChanged: "Key changed",
  Unsigned: "Not signed",
}

/** The same state inside a sentence. */
const STATE_IN_A_LINE: Readonly<Record<PluginPublisherState, string>> = {
  Bound: "configured",
  Trusted: "trusted",
  New: "not trusted yet",
  KeyChanged: "key changed",
  Unsigned: "not signed",
}

const keyPart = (publisher: PluginPublisher) =>
  publisher.fingerprint ? ` Key ${publisher.fingerprint}.` : ""

/** Every publisher an inspected install brings, the plugin's own first, once
 * each. When one namespace comes with two states, the one that asks the
 * person for something wins. */
export function publishersOf(report: PluginInspection): readonly PluginPublisher[] {
  const byNamespace = new Map<string, PluginPublisher>()
  for (const publisher of [report.publisher, ...report.brings.map(bring => bring.publisher)]) {
    const known = byNamespace.get(publisher.namespace)
    if (known === undefined || (!awaitsTrust(known) && awaitsTrust(publisher))) {
      byNamespace.set(publisher.namespace, publisher)
    }
  }
  return [...byNamespace.values()]
}

/** The publishers the person must trust before this install, in order. */
export const publishersToTrust = (report: PluginInspection): readonly PluginPublisher[] =>
  publishersOf(report).filter(awaitsTrust)

/** Whether the install takes an unsigned plugin, its own or one it brings. */
const bringsUnsigned = (report: PluginInspection): boolean =>
  [report.publisher, ...report.brings.map(bring => bring.publisher)].some(publisher => publisher.state === "Unsigned")

/**
 * The key change text (schema sheet row 15), one sentence per line. Joined
 * with spaces it is the sheet's text exactly.
 */
export function keyChangeLines(publisher: PluginPublisher): string[] {
  return [
    `The publisher key for ${publisher.namespace} changed.`,
    `Before: ${publisher.previousFingerprint ?? "unknown"}.`,
    `Now: ${publisher.fingerprint ?? "unknown"}.`,
    "This can mean the author replaced their key, or that someone else is publishing as them.",
    "Trust the new key only if the author told you about the change.",
  ]
}

const disclosureOf = (publisher: PluginPublisher): string =>
  publisher.disclosure?.trim() || UNSIGNED_DISCLOSURE

/** One line naming the plugin's own publisher, its state and its key. */
function publisherLine(publisher: PluginPublisher): string {
  const name = publisher.namespace
  switch (publisher.state) {
    case "Bound": return `Publisher ${name}, trusted by this device's configuration.${keyPart(publisher)}`
    case "Trusted": return `Publisher ${name}, trusted on this device.${keyPart(publisher)}`
    case "New": return `Publisher ${name}, not trusted yet.${keyPart(publisher)}`
    case "KeyChanged": return `Publisher ${name}: key changed. Before ${publisher.previousFingerprint ?? "unknown"}. Now ${publisher.fingerprint ?? "unknown"}.`
    case "Unsigned": return `Publisher ${name}, not signed.`
  }
}

/**
 * The approval report as plain lines, one fact per line. A surface shows the
 * confirmation message as text; the line breaks keep each fact apart.
 *
 * An unsigned plugin's disclosure comes first, before anything else to
 * approve; then who published it, what it asks for, and every plugin it
 * brings with that plugin's publisher.
 */
export function reviewMessage(report: PluginInspection): string {
  const lines: string[] = []
  if (report.publisher.state === "Unsigned") lines.push(disclosureOf(report.publisher))
  for (const bring of report.brings) {
    if (bring.publisher.state === "Unsigned") lines.push(`${bring.title ?? bring.id}: ${disclosureOf(bring.publisher)}`)
  }
  lines.push(publisherLine(report.publisher))
  const asks: string[] = []
  const warning = warningText(report.warning)
  if (warning) asks.push(`Warning: ${warning}`)
  const ports = [
    ...(report.ports.allowedTCPPorts.length ? [`TCP ${listOf(report.ports.allowedTCPPorts.map(String))}`] : []),
    ...(report.ports.allowedUDPPorts.length ? [`UDP ${listOf(report.ports.allowedUDPPorts.map(String))}`] : []),
  ]
  if (ports.length) asks.push(`Opens network ports: ${ports.join("; ")}.`)
  if (report.nativeUnits.length) asks.push(`Runs in the background: ${listOf(report.nativeUnits)}.`)
  for (const bring of report.brings) {
    const name = bring.title ?? bring.id
    const from = `from ${bring.publisher.namespace} (${STATE_IN_A_LINE[bring.publisher.state]})`
    if (bring.alreadyApproved) {
      asks.push(`Also uses ${name} ${from}. You approved it before.`)
    } else {
      const bringWarning = warningText(bring.warning)
      asks.push(`Also installs ${name} ${from}.${bringWarning ? ` Warning: ${bringWarning}` : ""}`)
    }
  }
  if (asks.length === 0) asks.push("It asks for no network ports and runs nothing in the background.")
  return [...lines, ...asks].join("\n")
}

/** What a publisher row under a review says below its state. */
function publisherDescription(publisher: PluginPublisher): string {
  const key = publisher.fingerprint ? `Key ${publisher.fingerprint}.` : ""
  switch (publisher.state) {
    case "Bound": return `${key} This device's configuration trusts it.`.trim()
    case "Trusted": return `${key} You trusted it on this device.`.trim()
    case "New": return `${key} Check it with the plugin maker before you trust it.`.trim()
    case "KeyChanged": return `Before ${publisher.previousFingerprint ?? "unknown"}. Now ${publisher.fingerprint ?? "unknown"}.`
    case "Unsigned": return disclosureOf(publisher)
  }
}

// ---------------------------------------------------------------- rows

const onOff = [
  { value: "true", label: "On" },
  { value: "false", label: "Off" },
] as const

/** What the person trusts with one press, and what it means. */
function trustRow(
  id: string,
  label: string,
  review: PluginReview,
  publisher: PluginPublisher,
  waiting: readonly PluginPublisher[],
): SurfaceSettingItem {
  const plugin = review.report.title ?? label
  const later = waiting.filter(other => other.namespace !== publisher.namespace).map(other => other.namespace)
  const then = later.length ? [`After this, ${plugin} also needs you to trust ${listOf(later)}.`] : []
  if (publisher.publicKey === undefined) {
    // Nothing to send: the host must report the key before anyone trusts it.
    return {
      id,
      label,
      value: "Check again",
      description: `Korri did not get the key of ${publisher.namespace}. Check the plugin again.`,
      interaction: { kind: "action", actionId: inspectActionId(review) },
    }
  }
  if (publisher.state === "KeyChanged") {
    return {
      id,
      label,
      value: "Trust the new key",
      description: `The publisher key for ${publisher.namespace} changed.`,
      interaction: {
        kind: "action",
        actionId: trustActionId(review, publisher.namespace),
        destructive: true,
        confirmation: {
          title: `Trust the new key for ${publisher.namespace}?`,
          message: [...keyChangeLines(publisher), ...then].join("\n"),
          confirmLabel: "Trust the new key",
        },
      },
    }
  }
  const own = publisher.namespace === review.report.publisher.namespace
  const needs = review.report.brings.find(bring => bring.publisher.namespace === publisher.namespace)
  return {
    id,
    label,
    value: `Trust ${publisher.namespace}`,
    description: `${publisher.namespace} is not trusted yet. Check its key, then trust it.`,
    interaction: {
      kind: "action",
      actionId: trustActionId(review, publisher.namespace),
      confirmation: {
        title: `Trust ${publisher.namespace}?`,
        message: [
          own || needs === undefined
            ? `${plugin} comes from ${publisher.namespace}. This device has not trusted it before.`
            : `${plugin} needs ${needs.title ?? needs.id} from ${publisher.namespace}. This device has not trusted it before.`,
          `Key ${publisher.fingerprint ?? "unknown"}.`,
          "Trust it only if the plugin maker gave you this key.",
          `Later plugins from ${publisher.namespace} must have the same key.`,
          ...then,
        ].join("\n"),
        confirmLabel: "Trust",
      },
    },
  }
}

/**
 * The second step, on the row the person pressed: trust each publisher that
 * asks for it, one press each, then the kept report, shown before anything
 * installs. The publishers the install brings follow it, one row each.
 */
const reviewRows = (
  id: string,
  label: string,
  review: PluginReview,
  description: string | undefined,
  wanted: boolean,
): SurfaceSettingItem[] => {
  const waiting = publishersToTrust(review.report)
  const next = waiting[0]
  const verb = wanted ? "Turn on" : "Turn off"
  const step: SurfaceSettingItem = next !== undefined
    ? trustRow(id, label, review, next, waiting)
    : {
        id,
        label,
        value: wanted ? "Review and turn on" : "Review and turn off",
        ...(description === undefined ? {} : { description }),
        interaction: {
          kind: "action",
          actionId: reviewActionId(review),
          // An unsigned plugin is the owner's own risk: the question warns.
          ...(bringsUnsigned(review.report) ? { destructive: true } : {}),
          confirmation: {
            title: `${verb} ${review.report.title ?? label}?`,
            message: reviewMessage(review.report),
            confirmLabel: verb,
          },
        },
      }
  return [
    step,
    ...publishersOf(review.report).map((publisher): SurfaceSettingItem => ({
      id: reviewPublisherRowId(review, publisher.namespace),
      label: `Publisher ${publisher.namespace}`,
      value: PUBLISHER_STATE_LABEL[publisher.state],
      description: publisherDescription(publisher),
    })),
  ]
}

const toggleRows = (
  pluginId: string,
  label: string,
  enabled: boolean,
  facts: PluginFacts,
  description: string | undefined,
): SurfaceSettingItem[] => {
  const install = installOf(facts, pluginId)
  const wanted = facts.needsReview[pluginId]
  if (wanted !== undefined && install !== undefined) {
    const target = { sourceUrl: install.sourceUrl, pluginId, releaseVersion: install.releaseVersion }
    const review = reviewFor(facts, target)
    if (review !== undefined) return reviewRows(toggleSettingId(pluginId), label, review, description, wanted)
    return [{
      id: toggleSettingId(pluginId),
      label,
      value: "Needs review",
      description: `It changed since you approved it. Check it again to turn it ${wanted ? "on" : "off"}.`,
      interaction: { kind: "action", actionId: inspectActionId(target) },
    }]
  }
  return [{
    id: toggleSettingId(pluginId),
    label,
    value: enabled ? "On" : "Off",
    ...(description === undefined ? {} : { description }),
    interaction: { kind: "choice", choices: onOff, selected: enabled ? "true" : "false" },
  }]
}

/** What a catalog row says about a publisher it cannot install from yet. */
function untrustedWords(publisher: PluginPublisher): string | undefined {
  switch (publisher.state) {
    case "Bound":
    case "Trusted":
      return undefined
    case "New": return `Publisher ${publisher.namespace} is not trusted yet.`
    case "KeyChanged": return `The publisher key for ${publisher.namespace} changed.`
    case "Unsigned": return `Not signed. Korri cannot confirm who made it.`
  }
}

const releaseRows = (
  target: ReleaseTarget,
  label: string,
  facts: PluginFacts,
  several: boolean,
  description: string | undefined,
): SurfaceSettingItem[] => {
  const review = reviewFor(facts, target)
  if (review !== undefined) {
    return reviewRows(releaseRowId(target), label, review, several ? `Version ${target.releaseVersion}` : undefined, true)
  }
  return [{
    id: releaseRowId(target),
    label,
    value: several ? `Install ${target.releaseVersion}` : "Install",
    ...(description === undefined ? {} : { description }),
    interaction: { kind: "action", actionId: inspectActionId(target) },
  }]
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
  const installedRows = () => fromHere.flatMap(install =>
    toggleRows(install.id, titles.get(install.id) ?? install.id, install.enabled, facts, undefined))
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
      rows.push(...toggleRows(
        pluginId,
        label,
        install?.enabled ?? installedEntry?.installed === PluginInstalledState.Enabled,
        facts,
        first.description,
      ))
      continue
    }
    // Install inspects first; trust, when the publisher needs it, comes after.
    const untrusted = untrustedWords(first.publisher)
    const several = entries.length > 1
    entries.forEach((entry, index) => {
      const about = several ? "Pick the version to install." : first.description
      rows.push(...releaseRows(
        { sourceUrl: url, pluginId, releaseVersion: entry.releaseVersion },
        label,
        facts,
        several,
        index === 0
          ? untrusted === undefined ? about : several ? `${untrusted} ${about}` : untrusted
          : undefined,
      ))
    })
  }
  // An install the catalog no longer lists still turns on and off.
  for (const install of fromHere) {
    if (!releases.has(install.id)) {
      rows.push(...toggleRows(install.id, titles.get(install.id) ?? install.id, install.enabled, facts, undefined))
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

/** One trusted publisher: its key, what is installed from it, and Revoke
 * when the owner trusted it here. A bound one belongs to the configuration. */
function publisherRow(entry: PluginPublisherEntry, titleOf: (id: string) => string): SurfaceSettingItem {
  const installed = entry.installed.length === 0
    ? "No plugins installed from it."
    : `${count(entry.installed.length, "plugin")} installed from it.`
  if (entry.state === "Bound") {
    return {
      id: publisherRowId(entry.namespace),
      label: entry.namespace,
      value: PUBLISHER_STATE_LABEL.Bound,
      description: `This device's configuration trusts it. Key ${entry.fingerprint}. ${installed}`,
    }
  }
  const names = entry.installed.map(titleOf)
  return {
    id: publisherRowId(entry.namespace),
    label: entry.namespace,
    value: "Revoke",
    description: `Trusted on this device. Key ${entry.fingerprint}. ${installed}`,
    interaction: {
      kind: "action",
      actionId: revokeActionId(entry.namespace),
      destructive: true,
      confirmation: {
        title: `Revoke ${entry.namespace}?`,
        message: (names.length === 0
          ? [
              `This device stops trusting ${entry.namespace}.`,
              `To install from ${entry.namespace} again, you check and trust its key again.`,
            ]
          : [
              `Plugins from ${entry.namespace} stop starting on this device: ${listOf(names)}.`,
              "You can still turn them off and remove them.",
              `To use them again, you check and trust the key of ${entry.namespace} again.`,
            ]).join("\n"),
        confirmLabel: "Revoke",
      },
    },
  }
}

/** The Publishers group, or nothing while the list is first read. */
function publishersGroup(facts: PluginFacts, titleOf: (id: string) => string): SurfaceSettingGroup | undefined {
  const publishers = facts.publishers
  if (publishers._tag === "Loading") return undefined
  if (publishers._tag === "Failed") {
    return {
      title: PUBLISHERS_GROUP,
      items: [{ id: PUBLISHERS_STATUS_ROW, label: "Can't read the publishers", description: publishers.message }],
    }
  }
  const { entries } = publishers.value
  return {
    title: PUBLISHERS_GROUP,
    items: entries.length === 0
      ? [{
          id: PUBLISHERS_STATUS_ROW,
          label: "No trusted publishers",
          description: "Korri asks you to trust a publisher when you install its first plugin.",
        }]
      : entries.map(entry => publisherRow(entry, titleOf)),
  }
}

export const PLUGINS_GROUP = "Plugins"
export const OFFICIAL_GROUP = "Official plugins"
export const REMOVED_GROUP = "Installed from removed repositories"
export const OTHER_GROUP = "Other installed plugins"
export const PUBLISHERS_GROUP = "Publishers"

/**
 * Settings groups for plugins, in this order:
 *
 *   Plugins                 the official repository, each added one, and the
 *                           row that adds another
 *   Official plugins        what the official repository offers
 *   <repository>            what one added repository offers, then Remove
 *   Installed from removed repositories
 *   Other installed plugins installed some other way, such as in the image
 *                           (`otherInstalls`, on or off)
 *   Publishers              every publisher this device trusts, with Revoke
 *                           for the ones its owner trusted
 *
 * `installed` is `SettingsSnapshot.plugins`, used only for the titles of
 * repository installs the catalog does not name. It lists enabled plugins
 * only, so it never decides which rows exist.
 */
export function pluginSettingGroups(
  facts: PluginFacts,
  installed: readonly PluginSetting[] | undefined,
): SurfaceSettingGroup[] {
  const titles = new Map((installed ?? []).map(plugin => [plugin.id, plugin.title]))
  const repositories = facts.repositories
  const knownTitles = new Map(titles)
  if (repositories._tag === "Ready") {
    for (const plugin of repositories.value.otherInstalls) if (plugin.title) knownTitles.set(plugin.id, plugin.title)
  }
  for (const catalog of Object.values(facts.catalogs)) {
    if (catalog._tag !== "Ready") continue
    for (const entry of catalog.entries) if (entry.title) knownTitles.set(entry.pluginId, entry.title)
  }
  const titleOf = (id: string) => knownTitles.get(id) ?? id
  const publishers = publishersGroup(facts, titleOf)
  const addRow: SurfaceSettingItem = {
    id: ADD_REPOSITORY_SETTING,
    label: "Add a repository",
    description: "Type the https:// address the plugin maker gave you.",
    interaction: { kind: "text", placeholder: "https://…", maxLength: 512 },
  }
  if (repositories._tag !== "Ready") {
    // The official row keeps its place and says where the list stands.
    const hub: SurfaceSettingGroup = {
      title: PLUGINS_GROUP,
      items: [
        {
          id: OFFICIAL_REPOSITORY_ROW,
          label: "Official plugins",
          value: repositories._tag === "Loading" ? "Checking…" : "Can't read the list",
          ...(repositories._tag === "Failed" ? { description: repositories.message } : {}),
        },
        addRow,
      ],
    }
    // The installs come with the list; until it is read there are none to show.
    return [hub, ...(publishers === undefined ? [] : [publishers])]
  }
  const { userAdded, repositoryInstalls, otherInstalls } = repositories.value
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
      addRow,
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
      items: removed.flatMap(install => toggleRows(
        install.id,
        titles.get(install.id) ?? install.id,
        install.enabled,
        facts,
        `From ${repositoryName(install.sourceUrl)}`,
      )),
    })
  }
  if (otherInstalls.length) {
    groups.push({
      title: OTHER_GROUP,
      items: otherInstalls.flatMap(plugin =>
        toggleRows(plugin.id, plugin.title ?? plugin.id, plugin.enabled, facts, undefined)),
    })
  }
  if (publishers !== undefined) groups.push(publishers)
  return groups
}
