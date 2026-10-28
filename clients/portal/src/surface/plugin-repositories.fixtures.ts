/** Fixtures for the plugin repository flow, in the generated wire types. */
import {
  PluginInstalledState,
  PluginPublisherState,
  type PluginCatalogEntry,
  type PluginInspection,
  type PluginPublisher,
  type PluginPublishers,
  type PluginRepositories,
  type RpcFailure,
} from "@contracts/generated/korrid"
import { UNSIGNED_DISCLOSURE } from "../korrid/client"
import type { PluginFacts, PluginRepositoryClient } from "./plugin-repositories"

export const OFFICIAL = "https://plugins.korri.example/official"
export const RETRO = "https://git.example/ada/retro-plugins"
export const BROKEN = "https://broken.example/catalog"
export const REMOVED = "https://old.example/plugins"

/* Publishers as an inspection reports them. Keys and fingerprints are made
 * up; each fingerprint is the host's display form of its key (sheet row 16). */
export const ADA_FINGERPRINT = "ada-plugins-1 QWRh IHNh bXBs ZSBr"
export const KORRI_FINGERPRINT = "korri-plugins-1 S29y cmkg c2Ft cGxl"
export const MALLORY_KEY = "mallory-1:TWFsbG9yeSBzYW1wbGUga2V5LCBub3QgcmVhbCE="
export const MALLORY_FINGERPRINT = "mallory-1 TWFs bG9y eSBz YW1w"
export const ADA_NEW_KEY = "ada-plugins-2:QWRhIG5ldyBzYW1wbGUga2V5LCBub3QgcmVhbCE="
export const ADA_NEW_FINGERPRINT = "ada-plugins-2 QWRh IG5l dyBz YW1w"
export const LIB_KEY = "lib-1:TGliIHNhbXBsZSBrZXksIG5vdCBhIHJlYWwga2V5IQ=="
export const LIB_FINGERPRINT = "lib-1 TGli IHNh bXBs ZSBr"

export const ada: PluginPublisher = { namespace: "@ada", fingerprint: ADA_FINGERPRINT, state: PluginPublisherState.Trusted }
export const korri: PluginPublisher = { namespace: "@korri", fingerprint: KORRI_FINGERPRINT, state: PluginPublisherState.Bound }
export const mallory: PluginPublisher = { namespace: "@mallory", fingerprint: MALLORY_FINGERPRINT, state: PluginPublisherState.New, publicKey: MALLORY_KEY }
export const adaKeyChanged: PluginPublisher = {
  namespace: "@ada",
  fingerprint: ADA_NEW_FINGERPRINT,
  state: PluginPublisherState.KeyChanged,
  previousFingerprint: ADA_FINGERPRINT,
  publicKey: ADA_NEW_KEY,
}
export const lib: PluginPublisher = { namespace: "@lib", fingerprint: LIB_FINGERPRINT, state: PluginPublisherState.New, publicKey: LIB_KEY }
export const pocket: PluginPublisher = { namespace: "@pocket", state: PluginPublisherState.Unsigned, disclosure: UNSIGNED_DISCLOSURE }

export const retroCatalog: PluginCatalogEntry[] = [
  { pluginId: "@ada:chiptune", title: "Chiptune", description: "Music player for tracker songs", releaseVersion: "1.0.0", publisher: { namespace: "@ada", state: PluginPublisherState.Trusted } },
  { pluginId: "@ada:crt", title: "CRT look", releaseVersion: "2.1.0", publisher: { namespace: "@ada", state: PluginPublisherState.Trusted } },
  { pluginId: "@ada:crt", title: "CRT look", releaseVersion: "2.0.3", publisher: { namespace: "@ada", state: PluginPublisherState.Trusted } },
  { pluginId: "@ada:netplay", title: "Netplay", releaseVersion: "0.4.0", installed: PluginInstalledState.Enabled, publisher: { namespace: "@ada", state: PluginPublisherState.Trusted } },
  { pluginId: "@mallory:miner", title: "Free coins", releaseVersion: "9.9.9", publisher: { namespace: "@mallory", state: PluginPublisherState.New } },
  { pluginId: "@pocket:dither", title: "Pocket dither", description: "Ordered dithering for small screens", releaseVersion: "0.1.0", publisher: { namespace: "@pocket", state: PluginPublisherState.New } },
]

/** Chiptune from a trusted publisher, bringing a plugin from the same one and
 * one from a bound one: nothing to trust, so the next step is the review. */
export const inspection: PluginInspection = {
  id: "@ada:chiptune",
  title: "Chiptune",
  approval: "sha256:4f1c9a7e0d2b",
  warning: "Plays sound over the network to paired speakers.",
  ports: { allowedTCPPorts: [7000, 7001], allowedUDPPorts: [5353] },
  nativeUnits: ["chiptune-daemon.service"],
  publisher: ada,
  brings: [
    { id: "@ada:audio-core", title: "Audio core", warning: "Can change the system volume.", alreadyApproved: false, publisher: ada },
    { id: "@korri:pipewire", title: "PipeWire bridge", warning: "", alreadyApproved: true, publisher: korri },
  ],
}

/** A first plugin from a publisher this device has not trusted. */
export const newPublisherInspection: PluginInspection = {
  id: "@mallory:miner",
  title: "Free coins",
  approval: "sha256:9a0b1c2d3e4f",
  warning: "",
  ports: { allowedTCPPorts: [], allowedUDPPorts: [] },
  nativeUnits: [],
  publisher: mallory,
  brings: [],
}

/** Chiptune signed by another key than the one this device pinned. */
export const keyChangedInspection: PluginInspection = { ...inspection, approval: "sha256:51de0a8c2b77", publisher: adaKeyChanged, brings: [] }

/** Chiptune bringing a plugin from a publisher this device has not trusted. */
export const bringsNewInspection: PluginInspection = {
  ...inspection,
  approval: "sha256:0c4e7d1f9b22",
  brings: [
    { id: "@lib:audio-core", title: "Audio core", warning: "Can change the system volume.", alreadyApproved: false, publisher: lib },
    { id: "@korri:pipewire", title: "PipeWire bridge", warning: "", alreadyApproved: true, publisher: korri },
  ],
}

/** An unsigned release: the disclosure, then the normal approval. */
export const unsignedInspection: PluginInspection = {
  id: "@pocket:dither",
  title: "Pocket dither",
  approval: "sha256:77a1e0c4d9b3",
  warning: "",
  ports: { allowedTCPPorts: [], allowedUDPPorts: [] },
  nativeUnits: [],
  publisher: pocket,
  brings: [],
}

/** The publishers this device trusts: one bound in its configuration, two
 * its owner trusted, one of them with nothing installed. */
export const trustedPublishers: PluginPublishers = {
  entries: [
    { namespace: "@korri", fingerprint: KORRI_FINGERPRINT, state: PluginPublisherState.Bound, installed: ["@korri:mgba", "@korri:retroarch"] },
    { namespace: "@ada", fingerprint: ADA_FINGERPRINT, state: PluginPublisherState.Trusted, installed: ["@ada:netplay"] },
    { namespace: "@pixel-collective-handheld", fingerprint: "pixel-collective-2026 UGl4 ZWwg Y29s bGVj", state: PluginPublisherState.Trusted, installed: [] },
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
  publishers: { _tag: "Loading" },
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
    pluginPublishers: method("pluginPublishers"),
    trustPublisher: method("trustPublisher"),
    revokePublisher: method("revokePublisher"),
  }
  return { client, calls }
}
