import { afterEach, describe, expect, it } from "bun:test"
import { SecretSettingStatus } from "@contracts/generated/korrid"
import {
  LaunchContributorKind,
  LaunchDisposition,
  LaunchForegroundKind,
  MoonlightImplementation,
  SessionControlFailureReason,
  SessionFreezerState,
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

  it("requests typed Moonlight resolution from korrid", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.moonlight.resolve",
          outcome: {
            _tag: "Available",
            payload: {
              transportId: "@korri:moonlight/moonlight",
              implementation: "artemis",
              sunshineApp: "Korri Stream",
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).moonlightResolve()

    expect(body).toEqual({ _tag: "app.moonlight.resolve", payload: {} })
    expect(outcome).toMatchObject({
      _tag: "Available",
      payload: { implementation: "artemis" },
    })
  })

  it("requests a signed Moonlight launch handoff from korrid", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.moonlight.launch.prepare",
          outcome: {
            _tag: "Ok",
            payload: {
              launchId: "0123456789abcdef0123456789abcdef",
              transportId: "@korri:moonlight/moonlight",
              implementation: "artemis",
              sunshineApp: "Korri Stream",
              hostUuid: "host-uuid",
              appId: 7,
              integrity: "opaque-integrity",
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).moonlightLaunchPrepare("host-uuid", 7, "skate3", "Skate 3")

    expect(body).toEqual({
      _tag: "app.moonlight.launch.prepare",
      payload: {
        hostUuid: "host-uuid",
        appId: 7,
        gameId: "skate3",
        title: "Skate 3",
      },
    })
    expect(outcome).toMatchObject({
      _tag: "Ok",
      payload: { launchId: "0123456789abcdef0123456789abcdef" },
    })
  })

  it("cancels only the named Moonlight launch reservation", async () => {
    let body: unknown
    globalThis.fetch = (async (_input, init) => {
      body = JSON.parse(String(init?.body))
      return new Response(
        JSON.stringify({
          _tag: "app.moonlight.launch.cancel",
          outcome: {
            _tag: "Ok",
            payload: { launchId: "0123456789abcdef0123456789abcdef" },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch

    const outcome = await createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    ).moonlightLaunchCancel("0123456789abcdef0123456789abcdef")

    expect(body).toEqual({
      _tag: "app.moonlight.launch.cancel",
      payload: { launchId: "0123456789abcdef0123456789abcdef" },
    })
    expect(outcome).toEqual({
      _tag: "Ok",
      payload: { launchId: "0123456789abcdef0123456789abcdef" },
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

  it("reports an unreachable brain for freeze and thaw", async () => {
    globalThis.fetch = (async () => {
      throw new Error("connection refused")
    }) as unknown as typeof fetch
    const client = createHttpKorridClient("http://127.0.0.1:43117", "capability")
    expect((await client.sessionFreeze("launch-1"))._tag).toBe("Err")
    expect((await client.sessionThaw("launch-1"))._tag).toBe("Err")
  })

  it("lists and invokes controls only for the supplied launch identity", async () => {
    const bodies: unknown[] = []
    globalThis.fetch = (async (_input, init) => {
      const body = JSON.parse(String(init?.body))
      bodies.push(body)
      const listing = body._tag === "app.session.controls"
      return new Response(
        JSON.stringify(
          listing
            ? {
                _tag: body._tag,
                outcome: {
                  _tag: "Ok",
                  payload: { launchId: body.payload.launchId, groups: [] },
                },
              }
            : {
                _tag: body._tag,
                outcome: {
                  _tag: "Ok",
                  payload: {
                    _tag: "Completed",
                    payload: { launchId: body.payload.launchId },
                  },
                },
              },
        ),
        { status: 200, headers: { "content-type": "application/json" } },
      )
    }) as typeof fetch
    const client = createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    )

    await client.sessionControls("launch-1")
    await client.invokeSessionControl("launch-1", "fill", {
      kind: "toggle",
      value: true,
    })

    expect(bodies).toEqual([
      {
        _tag: "app.session.controls",
        payload: { launchId: "launch-1" },
      },
      {
        _tag: "app.session.control.invoke",
        payload: {
          launchId: "launch-1",
          controlId: "fill",
          value: { kind: "toggle", value: true },
        },
      },
    ])
  })

  it("maps malformed successful control RPC payloads to calm unavailability", async () => {
    const responses = [
      { _tag: "wrong.tag", outcome: { _tag: "Ok", payload: { groups: [] } } },
      { _tag: "app.session.control.invoke", outcome: {
        _tag: "Ok",
        payload: { _tag: "UnknownResult" },
      } },
    ]
    globalThis.fetch = (async () => new Response(
      JSON.stringify(responses.shift()),
      { status: 200, headers: { "content-type": "application/json" } },
    )) as unknown as typeof fetch
    const client = createHttpKorridClient(
      "http://127.0.0.1:43117",
      "capability",
    )

    expect(await client.sessionControls("launch-1")).toEqual({
      _tag: "Err",
      payload: {
        reason: SessionControlFailureReason.Unavailable,
        message: "Gameplay controls are unavailable right now.",
      },
    })
    expect(await client.invokeSessionControl("launch-1", "fill")).toEqual({
      _tag: "Err",
      payload: {
        reason: SessionControlFailureReason.Unavailable,
        message: "That gameplay control is unavailable right now.",
      },
    })
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

describe("in-memory Moonlight policy", () => {
  it("keeps normal explicit user disable semantics", async () => {
    const client = createInMemoryKorridClient({
      moonlight: {
        _tag: "Available",
        payload: {
          transportId: "@fixture:moonlight/transport",
          implementation: MoonlightImplementation.Artemis,
          sunshineApp: "Fixture stream",
        },
      },
    })
    const settings = await client.settingsSnapshot()
    if (settings._tag !== "Ok") throw new Error("expected settings")

    expect((await client.moonlightResolve())._tag).toBe("Available")
    await client.updateSetting(
      settings.payload.revision,
      "@korri:moonlight",
      "false",
    )
    expect(await client.moonlightResolve()).toMatchObject({
      _tag: "Unavailable",
      payload: { code: "MoonlightUnavailable" },
    })
  })
})

describe("local games", () => {
  it("does not manufacture plugin-owned local routes by default", async () => {
    const client = createInMemoryKorridClient()
    expect(await client.localGames()).toEqual({
      _tag: "Ok",
      payload: { games: [] },
    })
    expect(await client.localGameLaunch("wl4")).toMatchObject({
      _tag: "Err",
      payload: { code: "LocalRomMissing" },
    })
  })

  it("returns caller-provided local launch instructions unchanged", async () => {
    const spec = {
      launchId: "fixture-launch",
      launcherId: "fixture-launcher",
      disposition: LaunchDisposition.Fresh,
      context: {
        gameId: "fixture",
        title: "Fixture",
        contributors: [
          { kind: LaunchContributorKind.Launcher, id: "fixture-launcher" },
        ],
        foreground: {
          kind: LaunchForegroundKind.Component,
          packageName: "dev.fixture.runtime",
          className: "dev.fixture.Main",
        },
      },
      component: { packageName: "dev.fixture.runtime", className: "dev.fixture.Main" },
      extras: { CONTENT: "/fixture/game.bin" },
      directories: [],
      files: [],
      integrity: "fixture-integrity",
    }
    const client = createInMemoryKorridClient({
      localGames: [{ id: "fixture", title: "Fixture", system: "Test" }],
      localLaunchSpecs: { fixture: spec },
    })

    expect(await client.localGameLaunch("fixture")).toEqual({
      _tag: "Ok",
      payload: spec,
    })
  })

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
