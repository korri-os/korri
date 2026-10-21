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
import { PluginRepositoryAddResult, type RpcFailure } from "@contracts/generated/korrid"
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
  withCatalog,
  withNeedsReview,
  wantedAfterReview,
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

  /**
   * One locked operation on `settingId`: the row saves while `call` runs.
   * A failure shows the host's words on the row, unless `refused` handles
   * it; a success goes to `done`, which sets the next status.
   */
  const locked = <T,>(
    settingId: string,
    call: () => Promise<{ readonly _tag: "Ok"; readonly payload: T } | { readonly _tag: "Err"; readonly payload: RpcFailure }>,
    done: (payload: T) => void | Promise<void>,
    refused?: (failure: RpcFailure) => boolean,
  ) => {
    if (!begin(settingId)) return
    void call().then(async outcome => {
      end()
      if (!host.alive()) return
      if (outcome._tag === "Err") {
        if (refused?.(outcome.payload) !== true) problem(settingId, outcome.payload.message)
        return
      }
      await done(outcome.payload)
    })
  }

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
    // One at a time: korrid serves plugin requests in turn, so parallel
    // catalog reads would queue ahead of a toggle the person presses next.
    for (const url of urls) {
      if (seq !== readSeq) return
      await readCatalog(url, seq)
    }
  }

  const addRepository = (typed: string) => {
    const url = httpsAddress(typed)
    if (url === undefined) {
      if (!host.busy.get()) problem(ADD_REPOSITORY_SETTING, INVALID_ADDRESS)
      return
    }
    // korrid fetches and checks the catalog before it saves anything.
    locked(ADD_REPOSITORY_SETTING, () => client.addPluginRepository(url), async result => {
      if (result === PluginRepositoryAddResult.AlreadyPresent) return problem(ADD_REPOSITORY_SETTING, ALREADY_ADDED)
      host.status({ _tag: "Idle" })
      await refresh()
    })
  }

  const removeRepository = (url: string) => {
    // NotFound: it is already gone, which is what the person asked for.
    locked(removeRepositoryActionId(url), () => client.removePluginRepository(url), async () => {
      host.status({ _tag: "Idle" })
      update(facts => withoutRepository(facts, url))
      await refresh()
    })
  }

  const inspect = (target: ReleaseTarget) => {
    // korrid downloads and verifies the release before it reports.
    locked(
      rowIdForRelease(host.facts(), target),
      () => client.inspectPlugin(target.sourceUrl, target.pluginId, target.releaseVersion),
      report => {
        update(facts => withReview(facts, target, report))
        host.status({ _tag: "Idle" })
      },
    )
  }

  /** Install exactly the reviewed release, then turn it on unless the person
   * was turning it off when the host called its approval stale. */
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
      // An install leaves the plugin off, which is all an "off" asked for.
      const wanted = wantedAfterReview(host.facts(), review.pluginId)
      update(facts => withoutReview(facts, target))
      const enabled = wanted ? await client.setPluginEnabled(review.pluginId, true) : undefined
      end()
      if (!host.alive()) return
      if (enabled?._tag === "Err") problem(toggleSettingId(review.pluginId), enabled.payload.message)
      else host.status({ _tag: "Idle" })
      host.changed()
    })()
  }

  const toggle = (pluginId: string, enabled: boolean) => {
    locked(
      toggleSettingId(pluginId),
      () => client.setPluginEnabled(pluginId, enabled),
      () => {
        host.status({ _tag: "Idle" })
        host.changed()
      },
      failure => {
        if (failure.code !== NEEDS_REVIEW_CODE || installOf(host.facts(), pluginId) === undefined) return false
        // The row turns into Needs review; its next press inspects again.
        update(facts => withNeedsReview(facts, pluginId, enabled))
        host.status({ _tag: "Idle" })
        return true
      },
    )
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

