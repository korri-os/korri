import { afterEach, describe, expect, it } from "bun:test"
import { SecretSettingStatus } from "@contracts/generated/korrid"
import {
  LaunchContributorKind,
  SessionControlFailureReason,
  SessionFreezerState,
  SourceCatalogState,
  SourceStreamControlState,
} from "@contracts/generated/korrid"
import {
  callKorrid,
  createDiscoverySnapshotPoller,
  createHttpKorridClient,
  createInMemoryKorridClient,
} from "./client"

const originalFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = originalFetch
})

describe("callKorrid", () => {
  it("sends the displayed launch identity through the existing stop wire field", async () => {
    let body: unknown
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        body = await request.json()
        return Response.json({
          _tag: "app.session.stop",
          outcome: { _tag: "Ok", payload: { phase: "stopped" } },
        })
      },
    })
    try {
      const client = createHttpKorridClient(server.url.origin, "capability")
      await client.sessionStop("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
      expect(body).toEqual({
        _tag: "app.session.stop",
        payload: { expectedLaunchId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
      })
    } finally {
      server.stop(true)
    }
  })

  it("rejects a missing stop identity without changing the in-memory session", async () => {
    for (const active of [undefined, { launchId: "current", gameId: "neverball" }]) {
      const client = createInMemoryKorridClient({ activeSession: active })
      const before = await client.sessionStatus()
      expect(await client.sessionStop()).toMatchObject({
        _tag: "Err", payload: { code: "ExpectedLaunchIdRequired" },
      })
      expect(await client.sessionStatus()).toEqual(before)
    }
  })

  it("does not stop a replacement in-memory session using an old launch identity", async () => {
    const active = { launchId: "current", gameId: "neverball" }
    const client = createInMemoryKorridClient({ activeSession: active })
    expect(await client.sessionStop("old")).toMatchObject({
      _tag: "Err", payload: { code: "StaleLaunchIdentity" },
    })
    expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: { active } })
    expect(await client.sessionStop("current")).toMatchObject({ _tag: "Ok" })
    expect(await client.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })

  it("correlates in-memory overlay handoff to the exact frozen launch and clears it on return", async () => {
    const active = { launchId: "launch-1", gameId: "one", phase: "running" }
    const client = createInMemoryKorridClient({ activeSession: active })

    expect(await client.sessionFreeze("launch-1")).toMatchObject({ _tag: "Ok" })
    expect(await client.sessionStatus()).toEqual({
      _tag: "Ok",
      payload: {
        active: { ...active, phase: "frozen" },
        overlay: { ...active, phase: "frozen" },
      },
    })

    expect(await client.sessionThaw("launch-1")).toMatchObject({ _tag: "Ok" })
    expect(await client.sessionStatus()).toEqual({
      _tag: "Ok",
      payload: { active },
    })
  })

  it("sends the per-server capability as a bearer token", async () => {
    let authorization: string | null = null
    globalThis.fetch = (async (_input, init) => {
      authorization = new Headers(init?.headers).get("authorization")
      return new Response(
        JSON.stringify({
          _tag: "system.health",
          outcome: { _tag: "Ok", payload: { version: "test" } },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const response = await callKorrid(
      "http://127.0.0.1:43117",
      "secret-capability",
      { _tag: "system.health", payload: {} },
    )

    if (authorization !== "Bearer secret-capability") {
      throw new Error(`unexpected authorization header: ${authorization}`)
    }
    expect(response._tag).toBe("system.health")
  })

  it("serializes the host-qualified prepare payload", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.session.prepare",
          outcome: { _tag: "Ok", payload: { gameId: "neverball" } },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).sessionPrepare("neverball", "zao")

    expect(body).toEqual({
      _tag: "app.session.prepare",
      payload: { gameId: "neverball", host: "zao" },
    })
  })

  it("stops only the supplied launch identity", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.session.stop",
          outcome: { _tag: "Ok", payload: { phase: "stopped" } },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).sessionStop("launch-1")

    expect(body).toEqual({
      _tag: "app.session.stop",
      payload: { expectedLaunchId: "launch-1" },
    })
  })

  it("freezes and thaws only the supplied launch identity", async () => {
    const bodies: unknown[] = []
    globalThis.fetch = (async (_input, init) => {
      const body = JSON.parse(String(init?.body))
      bodies.push(body)
      const state = body._tag === "app.session.freeze" ? "frozen" : "running"
      return new Response(
        JSON.stringify({
          _tag: body._tag,
          outcome: {
            _tag: "Ok",
            payload: { launchId: "launch-1", state, changed: true },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")
    const frozen = await client.sessionFreeze("launch-1")
    const thawed = await client.sessionThaw("launch-1")

    expect(bodies).toEqual([
      { _tag: "app.session.freeze", payload: { expectedLaunchId: "launch-1" } },
      { _tag: "app.session.thaw", payload: { expectedLaunchId: "launch-1" } },
    ])
    expect(frozen).toEqual({
      _tag: "Ok",
      payload: {
        launchId: "launch-1",
        state: SessionFreezerState.Frozen,
        changed: true,
      },
    })
    expect(thawed).toEqual({
      _tag: "Ok",
      payload: {
        launchId: "launch-1",
        state: SessionFreezerState.Running,
        changed: true,
      },
    })
  })

  it("lists and invokes gameplay controls with the same exact launch id", async () => {
    const bodies: unknown[] = []
    globalThis.fetch = (async (_input, init) => {
      const body = JSON.parse(String(init?.body))
      bodies.push(body)
      return new Response(JSON.stringify(body._tag === "app.session.controls"
        ? {
            _tag: body._tag,
            outcome: { _tag: "Ok", payload: { launchId: "launch-1", groups: [] } },
          }
        : {
            _tag: body._tag,
            outcome: { _tag: "Ok", payload: { launchId: "launch-1" } },
          }), { status: 200, headers: { "content-type": "application/json" } })
    }) as typeof fetch
    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")

    expect((await client.sessionControls("launch-1"))._tag).toBe("Ok")
    expect((await client.invokeSessionControl(
      "launch-1",
      "@korri:mgba/open-menu",
    ))._tag).toBe("Ok")

    expect(bodies).toEqual([
      { _tag: "app.session.controls", payload: { launchId: "launch-1" } },
      {
        _tag: "app.session.control.invoke",
        payload: { launchId: "launch-1", controlId: "@korri:mgba/open-menu" },
      },
    ])
  })

  it("reports an unreachable brain for freeze and thaw", async () => {
    globalThis.fetch = (async () => {
      throw new Error("connection refused")
    }) as unknown as typeof fetch
    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")
    expect((await client.sessionFreeze("launch-1"))._tag).toBe("Err")
    expect((await client.sessionThaw("launch-1"))._tag).toBe("Err")
  })

  it("asks for one peer's source status by device key", async () => {
    const bodies: unknown[] = []
    const key = "ab".repeat(32)
    globalThis.fetch = (async (_input, init) => {
      bodies.push(JSON.parse(String(init?.body)))
      return new Response(
        JSON.stringify({
          _tag: "app.source.status",
          outcome: {
            _tag: "Ok",
            payload: { catalog: "available", streamControl: "disabled" },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")
    const status = await client.sourceStatus(key)

    expect(bodies).toEqual([
      { _tag: "app.source.status", payload: { devicePublicKey: key } },
    ])
    expect(status).toEqual({
      _tag: "Ok",
      payload: {
        catalog: SourceCatalogState.Available,
        streamControl: SourceStreamControlState.Disabled,
      },
    })
  })

  it("reports an unreachable brain for source status", async () => {
    globalThis.fetch = (async () => {
      throw new Error("connection refused")
    }) as unknown as typeof fetch
    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")
    expect((await client.sourceStatus("ab".repeat(32)))._tag).toBe("Err")
  })

  it("aborts session status at its UI deadline", async () => {
    globalThis.fetch = ((_input, init) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(init.signal?.reason))
      })) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).sessionStatus(1)

    expect(outcome).toEqual({
      _tag: "Err",
      payload: {
        code: "StatusTimeout",
        message: "session status timed out",
      },
    })
  })

  it("aborts a stalled RPC at its deadline", async () => {
    globalThis.fetch = ((_input, init) =>
      new Promise((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => reject(init.signal?.reason))
      })) as typeof fetch

    let error: unknown
    try {
      await callKorrid(
        "http://127.0.0.1:43117",
        "capability",
        { _tag: "system.health", payload: {} },
        1,
      )
    } catch (caught) {
      error = caught
    }

    expect(error).toBeInstanceOf(DOMException)
    expect((error as DOMException).name).toBe("TimeoutError")
  })
})

describe("local games", () => {

  it("keeps healthy local games beside local configuration failures", async () => {
    const client = createInMemoryKorridClient({
      localGames: [
        { id: "wl4", title: "Wario Land 4", system: "Game Boy Advance" },
      ],
      localFailures: [
        {
          code: "LocalConfigReloadFailed",
          message: "library.yaml is malformed",
        },
      ],
    })

    expect(await client.localGames()).toEqual({
      _tag: "Ok",
      payload: {
        games: [
          { id: "wl4", title: "Wario Land 4", system: "Game Boy Advance" },
        ],
        failures: [
          {
            code: "LocalConfigReloadFailed",
            message: "library.yaml is malformed",
          },
        ],
      },
    })
  })
})

describe("in-memory source status", () => {
  const peerKey = "7b".repeat(32)
  const remoteGame = {
    id: "neverball",
    title: "Neverball",
    supportsRunnerSelection: false,
    source: { devicePublicKey: peerKey, label: "zao", isLocal: false },
  }

  it("answers only for peers that contributed a remote game", async () => {
    const client = createInMemoryKorridClient({ games: [remoteGame] })
    expect(await client.sourceStatus(peerKey)).toEqual({
      _tag: "Ok",
      payload: {
        catalog: SourceCatalogState.Available,
        streamControl: SourceStreamControlState.Enabled,
      },
    })
    expect(await client.sourceStatus("ff".repeat(32))).toMatchObject({
      _tag: "Err",
      payload: { code: "SourcePeerNotFound" },
    })
  })

  it("never treats the local device as a remote source", async () => {
    const client = createInMemoryKorridClient({
      games: [
        {
          id: "skate3",
          title: "Skate 3",
          supportsRunnerSelection: false,
          source: { devicePublicKey: peerKey, label: "browser", isLocal: true },
        },
      ],
    })
    expect((await client.sourceStatus(peerKey))._tag).toBe("Err")
  })

  it("mirrors a failing catalog as an unavailable source catalog", async () => {
    const client = createInMemoryKorridClient({
      behavior: "catalog-fail",
      games: [remoteGame],
    })
    expect(await client.sourceStatus(peerKey)).toMatchObject({
      _tag: "Ok",
      payload: { catalog: SourceCatalogState.Unavailable },
    })
  })
})

describe("discovery", () => {
  it("serializes receipt registration through the generated tag", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.discovery.registerReceipt",
          outcome: {
            _tag: "Ok",
            payload: {
              generation: "g1",
              state: { _tag: "Scanning", payload: {} },
              locations: [],
              diagnostics: [],
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).registerDiscoveryReceipt("receipt-1")

    expect(outcome).toMatchObject({
      _tag: "Ok",
      payload: { state: { _tag: "Scanning" } },
    })
    expect(body).toEqual({
      _tag: "app.discovery.registerReceipt",
      payload: { receipt: "receipt-1" },
    })
  })

  it("browser memory exposes the discovery lifecycle without raw paths", async () => {
    const client = createInMemoryKorridClient({
      discoveryReceipts: ["receipt-1"],
    })

    expect(await client.discoverySnapshot()).toMatchObject({
      _tag: "Ok",
      payload: { generation: "in-memory-0", state: { _tag: "Idle" } },
    })
    expect(await client.registerDiscoveryReceipt("receipt-1")).toMatchObject({
      _tag: "Ok",
      payload: { state: { _tag: "Scanning" } },
    })
    expect(await client.registerDiscoveryReceipt("receipt-1")).toMatchObject({
      _tag: "Err",
      payload: { code: "FolderSelectionReceiptUnknown" },
    })
    await new Promise(resolve => setTimeout(resolve, 1))
    expect(await client.discoverySnapshot()).toMatchObject({
      _tag: "Ok",
      payload: { state: { _tag: "Idle" } },
    })
  })

  it("polling publishes changed generations once and never overlaps requests", async () => {
    let generation = 0
    let active = 0
    let maxActive = 0
    const published: string[] = []
    const poller = createDiscoverySnapshotPoller(
      {
        async discoverySnapshot() {
          active += 1
          maxActive = Math.max(maxActive, active)
          await new Promise(resolve => setTimeout(resolve, 5))
          active -= 1
          return {
            _tag: "Ok",
            payload: {
              generation: `g${generation}`,
              state: { _tag: "Idle", payload: {} },
              locations: [],
              diagnostics: [],
            },
          }
        },
      },
      snapshot => published.push(snapshot.generation),
    )

    await Promise.all([poller.pollNow(), poller.pollNow()])
    await poller.pollNow()
    generation += 1
    await poller.pollNow()

    expect(maxActive).toBe(1)
    expect(published).toEqual(["g0", "g1"])
  })

  it("does not publish an in-flight polling result after disposal", async () => {
    let release: (() => void) | undefined
    const published: string[] = []
    const poller = createDiscoverySnapshotPoller(
      {
        async discoverySnapshot() {
          await new Promise<void>(resolve => {
            release = resolve
          })
          return {
            _tag: "Ok",
            payload: {
              generation: "late",
              state: { _tag: "Idle", payload: {} },
              locations: [],
              diagnostics: [],
            },
          }
        },
      },
      snapshot => published.push(snapshot.generation),
    )

    const pending = poller.pollNow()
    poller.dispose()
    release?.()
    await pending

    expect(published).toEqual([])
  })
})

describe("sensitive settings", () => {
  it("serializes SteamGridDB credential writes without expectedRevision", async () => {
    const bodies: unknown[] = []
    globalThis.fetch = (async (_input, init) => {
      bodies.push(JSON.parse(String(init?.body)))
      return new Response(
        JSON.stringify({
          _tag: "system.settings.steamgriddbCredential.set",
          outcome: { _tag: "Ok", payload: { status: "Configured" } },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).setSteamGridDbCredential("sgdb-secret-token")

    expect(outcome).toEqual({
      _tag: "Ok",
      payload: { status: SecretSettingStatus.Configured },
    })
    expect(bodies).toEqual([
      {
        _tag: "system.settings.steamgriddbCredential.set",
        payload: { token: "sgdb-secret-token" },
      },
    ])
    expect(JSON.stringify(bodies)).not.toContain("expectedRevision")
  })

  it("stores only configured/not-configured status in browser memory", async () => {
    const client = createInMemoryKorridClient()

    await client.setSteamGridDbCredential("sgdb-secret-token")
    const configured = await client.settingsSnapshot()
    await client.clearSteamGridDbCredential()
    const cleared = await client.settingsSnapshot()

    expect(configured).toMatchObject({
      _tag: "Ok",
      payload: { steamGridDbCredential: SecretSettingStatus.Configured },
    })
    expect(cleared).toMatchObject({
      _tag: "Ok",
      payload: { steamGridDbCredential: SecretSettingStatus.NotConfigured },
    })
    expect(JSON.stringify({ configured, cleared })).not.toContain(
      "sgdb-secret-token",
    )
  })
})
