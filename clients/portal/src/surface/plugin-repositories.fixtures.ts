/** Fixtures for the plugin repository flow, in the generated wire types. */
import {
  PluginInstalledState,
  type PluginCatalogEntry,
  type PluginInspection,
  type PluginRepositories,
  type RpcFailure,
} from "@contracts/generated/korrid"
import type { PluginFacts, PluginRepositoryClient } from "./plugin-repositories"

export const OFFICIAL = "https://plugins.korri.example/official"
export const RETRO = "https://github.com/ada/retro-plugins"
export const BROKEN = "https://broken.example/catalog"
export const REMOVED = "https://old.example/plugins"

export const retroCatalog: PluginCatalogEntry[] = [
  { pluginId: "@ada:chiptune", title: "Chiptune", description: "Music player for tracker songs", releaseVersion: "1.0.0", publisherTrusted: true },
  { pluginId: "@ada:crt", title: "CRT look", releaseVersion: "2.1.0", publisherTrusted: true },
  { pluginId: "@ada:crt", title: "CRT look", releaseVersion: "2.0.3", publisherTrusted: true },
  { pluginId: "@ada:netplay", title: "Netplay", releaseVersion: "0.4.0", installed: PluginInstalledState.Enabled, publisherTrusted: true },
  { pluginId: "@mallory:miner", title: "Free coins", releaseVersion: "9.9.9", publisherTrusted: false },
]

export const inspection: PluginInspection = {
  id: "@ada:chiptune",
  title: "Chiptune",
  approval: "sha256:4f1c9a7e0d2b",
  warning: "Plays sound over the network to paired speakers.",
  ports: { allowedTCPPorts: [7000, 7001], allowedUDPPorts: [5353] },
  nativeUnits: ["chiptune-daemon.service"],
  brings: [
    { id: "@ada:audio-core", title: "Audio core", warning: "Can change the system volume.", alreadyApproved: false },
    { id: "@korri:pipewire", title: "PipeWire bridge", warning: "", alreadyApproved: true },
  ],
}

/** No official repository unless `over` names one, as the wire sends it. */
export const repositories = (over: Partial<PluginRepositories> = {}): PluginRepositories => ({
  userAdded: [],
  repositoryInstalls: [],
  otherInstalls: [],
  ...over,
})

export const readyFacts = (over: Partial<PluginFacts> = {}): PluginFacts => ({
  repositories: { _tag: "Ready", value: repositories() },
  catalogs: {},
  reviews: {},
  needsReview: {},
  ...over,
})

export const ok = <T,>(payload: T) => ({ _tag: "Ok" as const, payload })
export const err = (code: string, message: string) =>
  ({ _tag: "Err" as const, payload: { code, message } satisfies RpcFailure })

type Call = readonly [keyof PluginRepositoryClient, ...unknown[]]

/** A scripted client: each method answers from `answers`, and every call is kept. */
export function fakePluginClient(answers: Partial<{
  [K in keyof PluginRepositoryClient]: (...args: Parameters<PluginRepositoryClient[K]>) => ReturnType<PluginRepositoryClient[K]> | Awaited<ReturnType<PluginRepositoryClient[K]>>
}>) {
  const calls: Call[] = []
  const method = <K extends keyof PluginRepositoryClient>(name: K) =>
    (async (...args: unknown[]) => {
      calls.push([name, ...args])
      const answer = answers[name] as ((...a: unknown[]) => unknown) | undefined
      if (!answer) throw new Error(`unexpected ${name}`)
      return answer(...args)
    }) as unknown as PluginRepositoryClient[K]
  const client: PluginRepositoryClient = {
    pluginRepositories: method("pluginRepositories"),
    addPluginRepository: method("addPluginRepository"),
    removePluginRepository: method("removePluginRepository"),
    pluginCatalog: method("pluginCatalog"),
    inspectPlugin: method("inspectPlugin"),
    installPlugin: method("installPlugin"),
    setPluginEnabled: method("setPluginEnabled"),
  }
  return { client, calls }
}
