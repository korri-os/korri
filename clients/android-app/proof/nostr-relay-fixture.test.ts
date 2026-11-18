import { afterEach, expect, test } from "bun:test";
import { hostEnvironment, startRelay } from "./nostr-relay-fixture";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";

const servers: ReturnType<typeof startRelay>[] = [];
afterEach(async () => { for (const server of servers.splice(0)) await server.stop(); });

async function client(url: string) {
  const socket = new WebSocket(url);
  const messages: unknown[][] = [];
  socket.addEventListener("message", (event) => messages.push(JSON.parse(String(event.data))));
  await new Promise<void>((resolve, reject) => {
    socket.addEventListener("open", () => resolve());
    socket.addEventListener("error", reject);
  });
  return { socket, messages, async send(value: unknown[]) {
    socket.send(JSON.stringify(value));
    if (value[0] !== "CLOSE") {
      const deadline = Date.now() + 2_000;
      const terminal = value[0] === "REQ" ? "EOSE" : "OK";
      while (!messages.some((message) => message[0] === terminal)) {
        if (Date.now() >= deadline) throw new Error(`Missing ${terminal}`);
        await Bun.sleep(5);
      }
    }
    return messages.splice(0);
  } };
}
test("host environment is an exact isolated allowlist despite hostile caller configuration", () => {
  process.env.KORRID_SYSTEMCTL = "/do-not-execute/systemctl";
  process.env.KORRID_PRIVATE_STATE_ROOT = "/do-not-touch/private";
  process.env.KORRI_LOCAL_STORAGE_ROOT = "/do-not-touch/readable";
  const env = hostEnvironment("/temporary/run", "/project", "/bounded/path", "nixpkgs=/bounded/nixpkgs");
  expect(env.PATH).toBe("/bounded/path");
  expect(env.NIX_PATH).toBe("nixpkgs=/bounded/nixpkgs");
  expect(env.KORRID_SYSTEMCTL).toBe("/project/clients/android-app/proof/federation-systemd-fixture.sh");
  expect(env.KORRID_PRIVATE_STATE_ROOT).toBe("/temporary/run/host/private");
  expect(env.KORRI_LOCAL_STORAGE_ROOT).toBeUndefined();
  expect(env.HOME).toBeUndefined();
  for (const [key, value] of Object.entries(env)) {
    if (key.endsWith("ROOT") || key.endsWith("DIRECTORY") || key.endsWith("SOCKET")) expect(value.startsWith("/temporary/run/")).toBe(true);
  }
});

test("executable Nix-shebang helper lists only isolated units with the actual host environment", async () => {
  const root = mkdtempSync(join(tmpdir(), "federation-helper-test-"));
  mkdirSync(join(root, "units"));
  const env = hostEnvironment(root, resolve(import.meta.dir, "../../.."), process.env.PATH ?? "", process.env.NIX_PATH ?? "");
  try {
    const child = Bun.spawn([env.KORRID_SYSTEMCTL!, "--system", "--no-ask-password", "list-units", "korri-game-*.service",
      "--state=activating,active,reloading,deactivating", "--plain", "--no-legend", "--no-pager"], { env, stdout: "pipe", stderr: "pipe" });
    const timer = setTimeout(() => child.kill("SIGKILL"), 15_000);
    try {
      const code = await child.exited;
      const stderr = await new Response(child.stderr).text();
      expect(code, stderr).toBe(0);
      expect(await new Response(child.stdout).text()).toBe("");
      expect(readFileSync(join(root, "units/calls"), "utf8")).toContain(" list-units ");
    } finally { clearTimeout(timer); }
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 20_000);

const event = (id: string, created_at: number, device = "b".repeat(64)) => ({
  id: id.repeat(64), pubkey: "a".repeat(64), created_at, kind: 30078,
  tags: [["d", `org.korri.device-owner:${device}`]], content: "", sig: "c".repeat(128),
});

test("real WebSocket EVENT acknowledgement, address replacement, filter, EOSE and CLOSE", async () => {
  const relay = startRelay(); servers.push(relay);
  const c = await client(relay.url);
  try {
    expect(await c.send(["EVENT", event("f", 1)])).toEqual([["OK", "f".repeat(64), true, ""]]);
    await c.send(["EVENT", event("e", 2)]);
    await c.send(["EVENT", event("d", 2)]); // Lowest ID wins equal NIP timestamps.
    await c.send(["EVENT", event("a", 1)]); // Stale cannot restore old binding.
    await c.send(["EVENT", event("c", 3, "c".repeat(64))]);
    expect(await c.send(["REQ", "r", { kinds: [30078], authors: ["a".repeat(64)], "#d": [`org.korri.device-owner:${"b".repeat(64)}`] }]))
      .toEqual([["EVENT", "r", event("d", 2)], ["EOSE", "r"]]);
    expect(await c.send(["CLOSE", "r"])).toEqual([]);
    expect(await c.send(["REQ", "none", { "#p": ["0".repeat(64)] }])).toEqual([["EOSE", "none"]]);
    expect(await c.send(["REQ", "limit", { kinds: [30078], limit: 1 }])).toHaveLength(2);
  } finally { c.socket.close(); }
});

test("relay refuses product payloads and unsupported commands; shutdown closes active sockets", async () => {
  const relay = startRelay(); servers.push(relay);
  const c = await client(relay.url);
  expect((await c.send(["EVENT", { ...event("a", 1), kind: 1 }]))[0]?.slice(0, 3))
    .toEqual(["OK", "a".repeat(64), false]);
  expect((await c.send(["EVENT", { ...event("b", 1), content: '{"games":[]}' }]))[0]?.slice(0, 3))
    .toEqual(["OK", "b".repeat(64), false]);
  const closed = new Promise<void>((resolve) => c.socket.addEventListener("close", () => resolve()));
  await relay.stop();
  await closed;
  expect(relay.events()).toEqual([]);
});
