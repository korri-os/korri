import { describe, expect, it } from "bun:test"
import { createServer } from "node:http"
import { once } from "node:events"
import { createPortalConnection } from "./portal-connection"

describe("portal connection", () => {
  it("uses token-authenticated catalog and hostless local session RPC with an actual local server", async () => {
    const requests: { path: string; authorization: string | null; body: unknown }[] = []
    const server = createServer(async (request, response) => {
      response.setHeader("access-control-allow-origin", request.headers.origin ?? "*")
      response.setHeader("access-control-allow-methods", "POST")
      response.setHeader("access-control-allow-headers", "content-type, authorization")
      if (request.method === "OPTIONS") {
        response.writeHead(204).end()
        return
      }
      let body = ""
      for await (const chunk of request) body += String(chunk)
      requests.push({
        path: request.url ?? "",
        authorization: request.headers.authorization ?? null,
        body: JSON.parse(body),
      })
      response.setHeader("content-type", "application/json")
      const outcomes = [
        { _tag: "app.catalog.snapshot", outcome: { _tag: "Ok", payload: { games: [] } } },
        { _tag: "app.session.prepare", outcome: { _tag: "Ok", payload: { gameId: "wl4", launchId: "local-1" } } },
        { _tag: "app.session.stop", outcome: { _tag: "Ok", payload: { phase: "stopped" } } },
      ]
      response.end(JSON.stringify(outcomes[requests.length - 1]))
    })
    server.listen(0, "127.0.0.1")
    await once(server, "listening")
    const address = server.address()
    if (address === null || typeof address === "string") throw new Error("No TCP listener")
    try {
      // The test preload installs Happy DOM's fetch, which drops explicit
      // cross-origin Authorization headers. Exercise the unmodified HTTP
      // client in a real Bun process rather than replacing global fetch.
      const child = Bun.spawn([process.execPath, "--eval", `
        import { createPortalConnection } from "./src/korrid/portal-connection.ts";
        const connection = createPortalConnection({
          korridPort: () => ${address.port},
          korridCapability: () => "private-test-capability",
        });
        process.stdout.write(JSON.stringify({
          catalog: await connection.korrid.catalogSnapshot(),
          prepared: await connection.korrid.sessionPrepare("wl4"),
          stopped: await connection.korrid.sessionStop("local-1"),
          system: await connection.bridge.systemInfo(),
        }));
      `], { stdout: "pipe", stderr: "pipe" })
      const output = JSON.parse(await new Response(child.stdout).text())
      expect(await child.exited).toBe(0)
      expect(output.catalog).toEqual({
        _tag: "Ok", payload: { games: [] },
      })
      expect(output.prepared).toEqual({
        _tag: "Ok", payload: { gameId: "wl4", launchId: "local-1" },
      })
      expect(output.stopped).toEqual({
        _tag: "Ok", payload: { phase: "stopped" },
      })
      expect(requests).toEqual([
        { path: "/rpc", authorization: "Bearer private-test-capability", body: { _tag: "app.catalog.snapshot", payload: {} } },
        { path: "/rpc", authorization: "Bearer private-test-capability", body: { _tag: "app.session.prepare", payload: { gameId: "wl4" } } },
        { path: "/rpc", authorization: "Bearer private-test-capability", body: { _tag: "app.session.stop", payload: { expectedLaunchId: "local-1" } } },
      ])
      expect(output.system).toEqual({
        _tag: "Unavailable", message: "Native device operations are unavailable in this shell.",
      })
    } finally {
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()))
    }
  })

  it("keeps an absent binding in browser development offline", async () => {
    const connection = createPortalConnection(undefined)
    expect(await connection.korrid.health()).toEqual({
      _tag: "Ok", payload: { version: "korrid-in-memory" },
    })
  })

  it.each([-1, 0, 65536, 1.5, NaN])("rejects invalid bound port %s without fixtures", port => {
    expect(() => createPortalConnection({
      korridPort: () => port,
      korridCapability: () => "private-test-capability",
    })).toThrow("The shell did not provide a valid korrid connection.")
  })

  it.each(["", "\nsecret", "secret\r\n"])("rejects an invalid bound capability", capability => {
    expect(() => createPortalConnection({
      korridPort: () => 43117,
      korridCapability: () => capability,
    })).toThrow("The shell did not provide a valid korrid connection.")
  })

  it("does not leak a binding exception or fall back to fixtures", () => {
    expect(() => createPortalConnection({
      korridPort: () => { throw new Error("secret-value") },
      korridCapability: () => "private-test-capability",
    })).toThrow("The shell did not provide a valid korrid connection.")
  })
})
