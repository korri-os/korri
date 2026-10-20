/**
 * The effects behind Settings' plugin rows: reading repositories and their
 * catalogs, adding and removing a repository, and the approval path.
 *
 * Approval is two presses. Install asks korrid to download and check one
 * release (`inspectPlugin`) and keeps the report. The row then offers Review
 * and turn on, whose confirmation shows that report. Only that confirmed
 * action installs, and it passes exactly the digest the report carried. An
 * install without a kept report does not happen.
 *
 * One plugin or settings operation runs at a time, as for every other setting:
 * `busy` is the portal's shared settings lock.
 */
import type { SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import {
  ALREADY_ADDED,
  INVALID_ADDRESS,
  NEEDS_REVIEW_CODE,
  checkingCatalogs,
  configuredRepositories,
  httpsAddress,
  installOf,
  isPluginSettingId,
  pluginActionFrom,
  pluginChangeFrom,
  removeRepositoryActionId,
  reviewFor,
  rowIdForRelease,
  toggleSettingId,
  variantOf,
  withCatalog,
  withNeedsReview,
  withoutRepository,
  withoutReview,
  withRepositories,
  withReview,
  ADD_REPOSITORY_SETTING,
  type PluginFacts,
  type PluginRepositoryClient,
  type ReleaseTarget,
} from "./plugin-repositories"

export interface PluginRepositoriesHost {
  /** Current plugin facts; the controller replaces them through `publish`. */
  readonly facts: () => PluginFacts
  readonly publish: (next: PluginFacts) => void
  readonly status: (next: SurfaceSettingsStatus) => void
  /** The shared settings lock. */
  readonly busy: { get(): boolean; set(value: boolean): void }
  /** A plugin was installed, turned on or off: settings, the library and the
   * plugin facts change. The host re-reads all three, including `refresh`. */
  readonly changed: () => void
  readonly alive: () => boolean
}

export interface PluginRepositoriesController {
  /** Read repositories, then every configured catalog. */
  refresh(): Promise<void>
  /** Handle a setting-local action. False when the id is not a plugin action. */
  run(actionId: string): boolean
  /** Handle a setting change. False when the id is not a plugin setting. */
  change(settingId: string, value: string): boolean
}

/** Whether this korrid client offers the plugin repository methods. */
export function supportsPluginRepositories<T extends object>(client: T): client is T & PluginRepositoryClient {
  const candidate = client as Partial<Record<keyof PluginRepositoryClient, unknown>>
  return (["pluginRepositories", "addPluginRepository", "removePluginRepository", "pluginCatalog",
    "inspectPlugin", "installPlugin", "setPluginEnabled"] as const)
    .every(method => typeof candidate[method] === "function")
}

export function createPluginRepositoriesController(
  client: PluginRepositoryClient,
  host: PluginRepositoriesHost,
): PluginRepositoriesController {
  let readSeq = 0
  const update = (change: (facts: PluginFacts) => PluginFacts) => {
    if (host.alive()) host.publish(change(host.facts()))
  }
  const problem = (settingId: string, message: string) =>
    host.status({ _tag: "Problem", settingId, message })

  /** Start one locked operation on `settingId`; false when another runs. */
  const begin = (settingId: string): boolean => {
    if (host.busy.get()) return false
    host.busy.set(true)
    host.status({ _tag: "Saving", settingId })
    return true
  }
  const end = () => host.busy.set(false)

  const readCatalog = async (url: string, seq: number) => {
    const outcome = await client.pluginCatalog(url)
    if (seq !== readSeq) return
    update(facts => withCatalog(facts, url, outcome))
  }

  const refresh = async () => {
    const seq = ++readSeq
    const outcome = await client.pluginRepositories()
    if (seq !== readSeq || !host.alive()) return
    update(facts => withRepositories(facts, outcome))
    if (outcome._tag !== "Ok") return
    const urls = configuredRepositories(outcome.payload)
    update(facts => checkingCatalogs(facts, urls))
    await Promise.all(urls.map(url => readCatalog(url, seq)))
  }

  const addRepository = (typed: string) => {
    const url = httpsAddress(typed)
    if (url === undefined) {
      if (!host.busy.get()) problem(ADD_REPOSITORY_SETTING, INVALID_ADDRESS)
      return
    }
    if (!begin(ADD_REPOSITORY_SETTING)) return
    // korrid fetches and checks the catalog before it saves anything.
    void client.addPluginRepository(url).then(async outcome => {
      end()
      if (!host.alive()) return
      if (outcome._tag === "Err") return problem(ADD_REPOSITORY_SETTING, outcome.payload.message)
      if (variantOf(outcome.payload) === "AlreadyPresent") return problem(ADD_REPOSITORY_SETTING, ALREADY_ADDED)
      host.status({ _tag: "Idle" })
      await refresh()
    })
  }

  const removeRepository = (url: string) => {
    const settingId = removeRepositoryActionId(url)
    if (!begin(settingId)) return
    void client.removePluginRepository(url).then(async outcome => {
      end()
      if (!host.alive()) return
      if (outcome._tag === "Err") return problem(settingId, outcome.payload.message)
      // NotFound: it is already gone, which is what the person asked for.
      host.status({ _tag: "Idle" })
      update(facts => withoutRepository(facts, url))
      await refresh()
    })
  }

  const inspect = (target: ReleaseTarget) => {
    const settingId = rowIdForRelease(host.facts(), target)
    if (!begin(settingId)) return
    // korrid downloads and verifies the release before it reports.
    void client.inspectPlugin(target.sourceUrl, target.pluginId, target.releaseVersion).then(outcome => {
      end()
      if (!host.alive()) return
      if (outcome._tag === "Err") return problem(settingId, outcome.payload.message)
      update(facts => withReview(facts, target, outcome.payload))
      host.status({ _tag: "Idle" })
    })
  }

  const approve = (target: ReleaseTarget) => {
    const review = reviewFor(host.facts(), target)
    // Nothing was inspected for this release, so nothing was shown to approve.
    if (review === undefined) return
    const settingId = rowIdForRelease(host.facts(), target)
    if (!begin(settingId)) return
    void (async () => {
      const installed = await client.installPlugin(
        review.sourceUrl,
        review.pluginId,
        review.releaseVersion,
        review.report.approval,
      )
      if (installed._tag === "Err") {
        end()
        if (host.alive()) problem(settingId, installed.payload.message)
        return
      }
      update(facts => withoutReview(facts, target))
      const enabled = await client.setPluginEnabled(review.pluginId, true)
      end()
      if (!host.alive()) return
      if (enabled._tag === "Err") problem(toggleSettingId(review.pluginId), enabled.payload.message)
      else host.status({ _tag: "Idle" })
      host.changed()
    })()
  }

  const toggle = (pluginId: string, enabled: boolean) => {
    const settingId = toggleSettingId(pluginId)
    if (!begin(settingId)) return
    void client.setPluginEnabled(pluginId, enabled).then(async outcome => {
      end()
      if (!host.alive()) return
      if (outcome._tag === "Err") {
        const install = installOf(host.facts(), pluginId)
        if (outcome.payload.code === NEEDS_REVIEW_CODE && install !== undefined) {
          // The row turns into Needs review; its next press inspects again.
          update(facts => withNeedsReview(facts, pluginId))
          host.status({ _tag: "Idle" })
          return
        }
        return problem(settingId, outcome.payload.message)
      }
      host.status({ _tag: "Idle" })
      host.changed()
    })
  }

  return {
    refresh,
    run(actionId) {
      const action = pluginActionFrom(actionId)
      if (action === undefined) return false
      switch (action._tag) {
        case "Remove":
          removeRepository(action.url)
          return true
        case "Inspect":
          inspect(action.target)
          return true
        case "Review":
          approve(action.target)
          return true
      }
    },
    change(settingId, value) {
      if (!isPluginSettingId(settingId)) return false
      const change = pluginChangeFrom(settingId, value)
      // A plugin row with a value it never offered: nothing to do.
      if (change === undefined) return true
      switch (change._tag) {
        case "AddRepository":
          addRepository(change.url)
          return true
        case "Toggle":
          toggle(change.pluginId, change.enabled)
          return true
      }
    },
  }
}

