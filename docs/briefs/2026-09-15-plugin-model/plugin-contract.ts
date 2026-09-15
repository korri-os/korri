/**
 * Korri plugin contract: v1-review (PROPOSAL, not the implemented SDK).
 * Data exports plus optional typed operations. No YAML plugin descriptor.
 * This file is a review contract, not a decision to generate Rust from TS.
 */
export type Json = null | boolean | number | string | Json[] | ObjectValue
export type ObjectValue = { [key: string]: Json }
export type PluginId = `@${string}:${string}`
export type RecordId = `${PluginId}/${string}`
export type FileKey = string
export type Handle = string // Host-issued, checked against owner/session.
export type Schema = ObjectValue // JSON Schema 2020-12; local refs only; runtime validation required.
export type Values = ObjectValue
export type Awaitable<T> = T | Promise<T>

export interface Diagnostic {
  code: string
  severity: "info" | "warning" | "error"
  message: string
  path?: string[]
}
export interface PluginError {
  code: string
  message: string
  retryable: boolean
  cancelled?: boolean
}
export interface FamilyRef { id: PluginId; version: number }
export interface Family {
  id: PluginId
  version: number
  title: string
  settings: Schema
  defaults: Values
}
export interface Runner {
  id: RecordId
  title: string
  systems: string[]
  family?: FamilyRef
  settings?: Schema // Otherwise use the package's settings export.
  defaults?: Values // Composed over package defaults for this runner.
  launch?: { "command-file": FileKey; args: string[] } // Fixed-process shortcut.
  controls?: string[] // Action IDs implemented by this runner's session adapter.
}
export interface SystemRecord { id: string; title: string }
export interface Provider { id: RecordId; title: string; systems?: string[] }
export interface Service { id: RecordId; unit: FileKey; ports?: { tcp?: number[]; udp?: number[] } }
export interface Control {
  id: RecordId
  title: string
  owner: { runner: RecordId } | { family: FamilyRef } | { plugin: PluginId }
  action: string
  interaction: "command" | "toggle" | "value"
  destructive?: boolean
  schema?: Schema // Value input schema when interaction is "value".
}
export interface FileRule {
  id: RecordId
  extensions: string[]
  system: string
  runners: RecordId[] // Candidates, not a default chosen by lexical order.
}
export interface CatalogEntry {
  id: RecordId
  title: string
  system?: string
  target: Target
  runner?: RecordId
}
export interface PluginData {
  name: string
  title: string
  description?: string
  systems?: Record<string, SystemRecord>
  runners?: Record<string, Runner>
  families?: Record<string, Family>
  providers?: Record<string, Provider>
  services?: Record<string, Service>
  controls?: Record<string, Control>
  discovery?: Record<string, FileRule>
  catalog?: Record<string, CatalogEntry>
  settings?: Schema
  defaults?: Values
  access?: ServiceGroup[]
  session?: { startup: "required" | "optional" }
}

// Public data exports match PluginData fields. A source generator can supply
// omitted declarations. The EFFECTIVE module/artifact must have identity.
export type PluginModule = PluginData & { handlers?: Handlers }
export type ServiceGroup = "content" | "http" | "credentials" | "state" | "native" | "artifacts" | "jobs" | "resources"

export interface Facts {
  request: string
  plugin: PluginId
  generation: string // Selected immutable artifact.
  platform: "linux" | "android"
  device: string
  account: string
  session?: Handle
  paths: { data: string; cache: string; temporary: string; saves: string; states: string }
}
export interface Context<I> {
  input: Readonly<I>
  settings: Readonly<Values>
  files: Readonly<Record<FileKey, string>>
  context: Readonly<Facts>
  services: HostServices
}
export interface HostServices {
  cancelled(): boolean
  log(diagnostic: Diagnostic): void
  content?: {
    readText(evidence: Handle): Promise<string | undefined>
    readBytes(evidence: Handle): Promise<Uint8Array | undefined>
  }
  http?: {
    request(request: {
      url: string; method?: string; headers?: Record<string, string>; body?: string
    }): Promise<{ status: number; url: string; headers: Record<string, string>; body: Uint8Array }>
  }
  credentials?: { get(name: string): Promise<string | undefined> }
  state?: {
    readText(path: string): Promise<string | undefined>
    writeText(path: string, content: string): Promise<void>
    ensureDirectory(path: string): Promise<void>
    exists(path: string): Promise<boolean>
  }
  artifacts?: {
    download(request: Extract<DownloadResolution, { status: "ready" }>, job: Handle): Promise<Artifact>
    // Resolves only handles allocated to this request/job; enables native installers.
    path(handle: Handle): Promise<string>
  }
  native?: {
    // File key is an approved helper artifact. Processes are host tracked.
    start(request: { file: FileKey; args: string[]; env?: Record<string, string>; cwd?: string }): Promise<Handle>
    wait(process: Handle): Promise<{ exit: number; stdout: string; stderr: string }>
    // Only helper handles registered for a native control protocol support this.
    // Sidecars can retain sockets/subscriptions; TS receives plain results.
    call(resource: Handle, method: string, input: Json): Promise<Json>
    stop(process: Handle): Promise<void>
  }
  resources?: {
    release(resource: Handle): Promise<void>
  }
  jobs?: {
    progress(job: Handle, update: Progress): void
    // IDs are allocated by the host before install/acquire operations start.
  }
}

export type Target =
  | { kind: "file"; evidence: Handle; path: string }
  | { kind: "files"; root: string; files: { id: string; path: string; evidence: Handle }[]; part?: string }
  | { kind: "directory"; evidence: Handle; path: string }
  | { kind: "url"; url: string }
  | { kind: "provider-ref"; provider: PluginId; ref: string }
  | { kind: "profile"; provider: PluginId; id: string }
  | { kind: "executable"; file: FileKey }
export interface Selection {
  runner: RecordId
  game?: string
  release?: string
  system?: string
  target: Target
}
export interface Preferences {
  video?: { fullscreen?: boolean; resolution?: { width: number; height: number }; "aspect-ratio"?: string }
  audio?: { volume?: number } // 0..100, not a universal dB conversion.
  latency?: "low" | "normal"
}
export interface LegacyOverrides {
  args?: { prepend?: string[]; append?: string[]; replace?: string[] }
  config?: { prepend?: string; append?: string; replace?: string }
}
export interface LaunchPlan {
  command: string
  args: string[]
  env?: Record<string, string>
  "env-unset"?: string[]
  cwd?: string
  directories?: string[]
  files?: { path: string; content: string; sensitive?: boolean }[]
  resources?: Handle[]
  metadata?: ObjectValue // Plugin-owned JSON, never executable callbacks.
  diagnostics?: Diagnostic[]
}
export interface SessionInput {
  session: Handle
  selection: Selection
  process?: Handle // Can be absent after partial startup failure.
  resources: Handle[]
  plan: LaunchPlan
}
export type StopReason = "exit" | "user" | "cancelled" | "startup-failed" | "timeout" | "host-shutdown"
export interface Progress { phase: string; completed?: number; total?: number; message?: string }
export interface Page<T> { items: T[]; cursor?: string; diagnostics?: Diagnostic[] }
export interface Query { text?: string; system?: string; cursor?: string; limit: number }
export interface Reference { provider: PluginId; id: string }
export interface Claim {
  ref: Reference; title: string; system?: string; target?: Target
  description?: string; artifacts?: ArtifactRef[]
}
export interface ArtifactRef { ref: Reference; title: string; mediaType?: string }
export type DownloadResolution =
  | { status: "ready"; url: string; filename?: string; mediaType?: string; headers?: Record<string, string>; sha256?: string }
  | { status: "user-action"; message: string; url?: string }
  | { status: "unsupported"; message: string }
export interface Artifact { handle: Handle; filename: string; bytes: number; sha256: string }
export type JobStatus =
  | { state: "pending" | "running" | "cancelling"; progress?: Progress }
  | { state: "completed"; artifacts?: Artifact[]; targets?: Target[]; diagnostics?: Diagnostic[] }
  | { state: "failed"; error: PluginError }
  | { state: "cancelled"; diagnostics?: Diagnostic[] }
export interface EvidenceFile { id: Handle; "relative-path": string; name: string; extension: string }
export interface Observation { evidence: Handle[]; target: Target; title?: string; system?: string; runners?: RecordId[]; confidence: "high" | "medium" | "low" }
export interface Choice { value: Json; label: string; available: boolean; reason?: string }
export interface SettingsDescription { schema: Schema; revision: string; diagnostics?: Diagnostic[] }
export interface StreamEndpoint { id: Reference; title: string; target: Target; capabilities: string[] }
export interface ControlState { action: string; available: boolean; value?: Json; reason?: string }

/** Closed operation map. Plugin-specific settings/payload schemas supplement it. */
export interface Operations {
  "preferences.map": {
    input: { runner: RecordId; preferences: Preferences }
    output: { settings: Values; handled: string[]; diagnostics: Diagnostic[] }
  }
  "settings.describe": { input: { selection: Selection }; output: SettingsDescription }
  "settings.options": {
    input: { selection: Selection; path: string[] }
    output: { choices: Choice[]; revision: string; expires?: string; diagnostics?: Diagnostic[] }
  }
  "settings.validate": {
    input: { selection: Selection; values: Values; revision?: string }
    output: { valid: boolean; diagnostics: Diagnostic[] }
  }
  "discovery.scan": {
    input: { source: string; files: EvidenceFile[] }
    output: { observations: Observation[]; diagnostics: Diagnostic[] }
  }
  "catalog.list": { input: Query; output: Page<CatalogEntry> }
  "claims.search": { input: Query; output: Page<Claim> }
  "claims.details": { input: Reference; output: Claim }
  "claims.parse-url": { input: { url: string }; output: { ref: Reference | null; diagnostics?: Diagnostic[] } }
  "provider.validate": {
    input: { provider: PluginId }
    output: { status: "ready" | "unavailable" | "user-action"; diagnostics: Diagnostic[] }
  }
  "artifact.resolve-download": { input: ArtifactRef; output: DownloadResolution }
  "artifact.acquire": {
    input: { job: Handle; download: Extract<DownloadResolution, { status: "ready" }> }
    output: { job: Handle; status: JobStatus }
  }
  "install.request": {
    input: { job: Handle; artifacts: Artifact[]; destination: Handle; options: Values }
    output: { job: Handle; status: JobStatus }
  }
  "job.status": { input: { job: Handle }; output: JobStatus }
  "job.cancel": { input: { job: Handle }; output: { acknowledged: boolean; diagnostics?: Diagnostic[] } }
  "runtime.resolve": {
    input: { selection: Selection }
    output: { ready: boolean; files: Record<string, string>; missing: string[]; diagnostics: Diagnostic[] }
  }
  "launch.prepare": { input: { selection: Selection; overrides: LegacyOverrides }; output: LaunchPlan }
  "launch.compose": { input: { selection: Selection; plan: LaunchPlan }; output: LaunchPlan }
  "session.started": { input: SessionInput; output: { ready: boolean; resources: Handle[]; diagnostics: Diagnostic[] } }
  "session.describe": { input: SessionInput; output: { controls: ControlState[]; diagnostics?: Diagnostic[] } }
  "session.control": {
    input: SessionInput & { action: string; value?: Json }
    output: { status: "applied" | "rejected" | "unsupported"; actual?: Json; diagnostics: Diagnostic[] }
  }
  "session.stopping": { input: SessionInput & { reason: StopReason }; output: { diagnostics: Diagnostic[] } }
  "session.cleanup": {
    input: SessionInput & { reason: StopReason }
    output: { released: Handle[]; residual: Handle[]; diagnostics: Diagnostic[] }
  }
  "stream.discover": { input: Query; output: Page<StreamEndpoint> }
  "diagnostics.collect": { input: { session?: Handle }; output: { diagnostics: Diagnostic[] } }
}
export type Operation = keyof Operations
export type Handler<K extends Operation> = (context: Context<Operations[K]["input"]>) => Awaitable<Operations[K]["output"]>
export type Handlers = { [K in Operation]?: Handler<K> }

/** Generated artifact index. Production serialization/validation remains work. */
export interface Manifest {
  contract: "korri.plugin/v1-review"
  id: PluginId
  declaration: PluginData
  operations: Operation[]
  entry?: "plugin.ts"
  sources: string[] // Package-relative explicit graph inventory; package.json included.
  files: Record<FileKey, string> // Runtime store paths, not the source inventory.
}
