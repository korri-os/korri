// Test-process-only NIP-01 relay and exact fixture lifecycle. Never imported by product code.
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import { createConnection } from "node:net";
import type { ServerWebSocket, Subprocess } from "bun";

type Event = { id: string; pubkey: string; created_at: number; kind: number; tags: string[][]; content: string; sig: string };
type Filter = { kinds?: number[]; authors?: string[]; since?: number; limit?: number; "#d"?: string[]; "#p"?: string[] };
const tag = (event: Event, name: string) => event.tags.find((tag) => tag[0] === name)?.[1];
function accepted(value: Event): boolean {
  if (!value || value.kind !== 30078 || !/^[a-f0-9]{64}$/.test(value.id) ||
      !/^[a-f0-9]{64}$/.test(value.pubkey) || !/^[a-f0-9]{128}$/.test(value.sig) ||
      !Number.isSafeInteger(value.created_at) || !Array.isArray(value.tags) ||
      !value.tags.every((tag) => Array.isArray(tag) && tag.every((part) => typeof part === "string"))) return false;
  const address = tag(value, "d") ?? "";
  // The actual consumers verify signatures. The relay accepts only the two discovery
  // protocols, not catalog/RPC/video or plaintext in an endpoint envelope.
  return (address.startsWith("org.korri.device-owner:") && value.content === "") ||
    (address.startsWith("org.korri.endpoint:") && typeof value.content === "string" &&
      /^[A-Za-z0-9+/=]+$/.test(value.content));
}
function matches(event: Event, filter: Filter): boolean {
  return (!filter.kinds || filter.kinds.includes(event.kind)) &&
    (!filter.authors || filter.authors.includes(event.pubkey)) &&
    (filter.since === undefined || event.created_at >= filter.since) &&
    ["d", "p"].every((name) => {
      const values = filter[`#${name}` as "#d" | "#p"];
      return !values || event.tags.some((tag) => tag[0] === name && values.includes(tag[1]!));
    });
}
export function startRelay() {
  const stored = new Map<string, Event>();
  const sockets = new Set<ServerWebSocket<undefined>>();
  let stopped = false;
  const server = Bun.serve<undefined>({
    hostname: "127.0.0.1", port: 0,
    fetch(request, server) {
      if (new URL(request.url).pathname === "/" && server.upgrade(request)) return;
      return new Response("WebSocket discovery only", { status: 400 });
    },
    websocket: {
      maxPayloadLength: 64 * 1024,
      open(socket) { sockets.add(socket); },
      close(socket) { sockets.delete(socket); },
      message(socket, raw) {
        try {
          const message = JSON.parse(String(raw));
          if (!Array.isArray(message)) throw new Error("array required");
          if (message[0] === "EVENT" && message.length === 2) {
            const event: Event = message[1];
            const valid = accepted(event);
            if (valid) {
              const key = `${event.kind}:${event.pubkey}:${tag(event, "d")}`;
              const old = stored.get(key);
              if (!old || event.created_at > old.created_at || (event.created_at === old.created_at && event.id < old.id)) {
                if (stored.size >= 256 && !old) throw new Error("fixture event bound");
                stored.set(key, event);
              }
            }
            socket.send(JSON.stringify(["OK", event?.id ?? "", valid, valid ? "" : "discovery events only"]));
          } else if (message[0] === "REQ" && message.length === 3 && typeof message[1] === "string") {
            const filter: Filter = message[2];
            const limit = Math.min(256, Math.max(0, filter.limit ?? 256));
            const events = [...stored.values()].filter((event) => matches(event, filter))
              .sort((a, b) => b.created_at - a.created_at || a.id.localeCompare(b.id)).slice(0, limit);
            for (const event of events) socket.send(JSON.stringify(["EVENT", message[1], event]));
            socket.send(JSON.stringify(["EOSE", message[1]]));
          } else if (message[0] !== "CLOSE" || message.length !== 2) {
            throw new Error("unsupported command");
          }
        } catch { socket.close(1008, "invalid fixture protocol"); }
      },
    },
  });
  const port = server.port!;
  return {
    url: `ws://127.0.0.1:${port}`, port,
    events: () => structuredClone([...stored.values()]),
    async stop() {
      if (stopped) return;
      stopped = true;
      for (const socket of sockets) socket.terminate();
      // Bun 1.3.11's stop promise stays pending after a WebSocket upgrade even
      // after all sockets close. Stop synchronously and verify the real listener.
      void server.stop(true);
      const deadline = Date.now() + 2_000;
      for (;;) {
        const closed = await new Promise<boolean>((resolve, reject) => {
          const socket = createConnection({ host: "127.0.0.1", port });
          socket.setTimeout(200, () => { socket.destroy(); reject(new Error("listener probe timed out")); });
          socket.once("connect", () => { socket.destroy(); resolve(false); });
          socket.once("error", (error: NodeJS.ErrnoException) => {
            if (error.code === "ECONNREFUSED") resolve(true);
            else reject(error);
          });
        });
        if (closed) break;
        if (Date.now() >= deadline) throw new Error("relay listener did not stop");
        await Bun.sleep(10);
      }
    },
  };
}

// Explicit allowlist: no caller KORRID_*, readable root, live socket, credential,
// unit policy, HOME, or XDG state can enter the host process.
export function hostEnvironment(root: string, project: string, path: string, nixPath: string): Record<string, string> {
  if (!isAbsolute(root) || !isAbsolute(project)) throw new Error("absolute fixture paths required");
  const uid = String(process.getuid!()), gid = String(process.getgid!());
  const helper = join(project, "clients/android/test/federation-systemd-fixture.sh");
  return {
    // The executable helper has a Nix shebang. Like PATH, NIX_PATH is toolchain
    // configuration, not host authority. Without it nix-shell cannot find nixpkgs.
    PATH: path, NIX_PATH: nixPath,
    KORRI_FEDERATION_UNIT_ROOT: join(root, "units"),
    KORRID_SYSTEMD_RUN: helper, KORRID_SYSTEMCTL: helper,
    KORRID_PRIVATE_STATE_ROOT: join(root, "host/private"),
    KORRID_SUNSHINE_PRIVATE_STATE_ROOT: join(root, "sunshine"),
    KORRID_CONTROL_DIRECTORY: join(root, "control"),
    KORRID_CONTROL_SOCKET: join(root, "control/control.sock"),
    KORRID_COMPOSITOR_CONTROL_DIRECTORY: join(root, "compositor"),
    KORRID_CERTIFICATE_CONTROL_DIRECTORY: join(root, "certificate"),
    KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET: join(root, "certificate/missing.sock"),
    KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID: gid,
    KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID: uid,
    KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID: gid,
    KORRID_RUNTIME_UID: uid, KORRID_RUNTIME_GID: gid,
  };
}

export async function startAcceptance(root: string, project: string, executable: string) {
  if (!isAbsolute(root) || !isAbsolute(executable) || existsSync(join(root, "host"))) throw new Error("fresh fixture root required");
  mkdirSync(join(root, "units"));
  const relay = startRelay();
  // Hold an actual loopback port that rejects requests, avoiding races with an
  // unrelated service occupying a guessed unavailable port. It is not a relay.
  const unavailable = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response("Unavailable", { status: 503 }) });
  const relays = [`ws://127.0.0.1:${unavailable.port}`, relay.url];
  const token = crypto.randomUUID();
  let host: Subprocess | undefined;
  let hostPort = 0;
  let hostKey = "";
  let busy = false;
  let relayStopped = false;
  const environment = hostEnvironment(root, project, process.env.PATH ?? "", process.env.NIX_PATH ?? "");
  // Paths only, no private bytes. Evidence for the harness isolation assertion.
  writeFileSync(join(root, "host-environment.json"), JSON.stringify(environment));
  async function startHost() {
    if (host) throw new Error("host already running");
    rmSync(join(root, "host/ready"), { force: true });
    const log = openSync(join(root, hostPort ? "host-restart.log" : "host.log"), "a", 0o600);
    try {
      host = Bun.spawn([executable, join(root, "host"), String(hostPort), JSON.stringify(relays)], {
        env: environment, stdout: log, stderr: log,
      });
    } finally { closeSync(log); }
    const deadline = Date.now() + 30_000;
    while (!existsSync(join(root, "host/ready"))) {
      if (host.exitCode !== null || Date.now() >= deadline) throw new Error("host startup failed or timed out");
      await Bun.sleep(50);
    }
    const [port, key] = readFileSync(join(root, "host/ready"), "utf8").trim().split(" ");
    if (!/^\d+$/.test(port!) || !/^[a-f0-9]{64}$/.test(key!) || (hostPort && (Number(port) !== hostPort || key !== hostKey))) throw new Error("host authority changed");
    hostPort = Number(port); hostKey = key!;
    console.log(`host ready port=${hostPort} device=${hostKey}`);
  }
  async function stopHost() {
    if (!host) throw new Error("host already stopped");
    const process = host;
    process.kill("SIGTERM");
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const code = await Promise.race([process.exited, new Promise<never>((_, reject) => {
        timer = setTimeout(() => { process.kill("SIGKILL"); reject(new Error("host did not join within 10s")); }, 10_000);
      })]);
      if (code !== 0) throw new Error(`host exit ${code}`);
      host = undefined;
      console.log("host stopped with joined discovery");
    } finally { clearTimeout(timer); }
  }
  const control = Bun.serve({
    hostname: "127.0.0.1", port: 0,
    async fetch(request) {
      if (request.method !== "POST" || request.headers.get("Authorization") !== `Bearer ${token}`) return new Response(null, { status: 403 });
      if (busy) return new Response(null, { status: 409 });
      const route = new URL(request.url).pathname;
      if (!["/evidence", "/relay/stop", "/host/stop", "/host/start"].includes(route) || new URL(request.url).search || await request.text() !== "{}") return new Response(null, { status: 400 });
      busy = true;
      try {
        if (route === "/relay/stop") { await relay.stop(); relayStopped = true; console.log("relay stopped"); }
        if (route === "/host/stop") await stopHost();
        if (route === "/host/start") await startHost();
        const events = relay.events();
        return Response.json({ hostPort, hostKey, relayStopped, events });
      } catch (error) { console.error(error); return new Response("fixture lifecycle failure", { status: 500 }); }
      finally { busy = false; }
    },
  });
  async function stop() {
    await control.stop(true);
    await relay.stop();
    await unavailable.stop(true);
    if (host) await stopHost();
  }
  try { await startHost(); } catch (error) { await stop(); throw error; }
  return { relayPort: relay.port, unavailablePort: unavailable.port!, controlPort: control.port!, token,
    hostPort, hostKey, stop };
}

if (import.meta.main) {
  const [root, project, executable] = process.argv.slice(2);
  if (!root || !project || !executable || process.argv.length !== 5) throw new Error("root project executable required");
  const fixture = await startAcceptance(root, project, executable);
  let stopping = false;
  const stop = async () => {
    if (stopping) return;
    stopping = true;
    try { await fixture.stop(); process.exit(0); } catch (error) { console.error(error); process.exit(1); }
  };
  process.on("SIGTERM", stop); process.on("SIGINT", stop);
  writeFileSync(join(root, "fixture-ready.tmp"), `${fixture.hostPort} ${fixture.hostKey} ${fixture.relayPort} ${fixture.unavailablePort} ${fixture.controlPort} ${fixture.token}\n`);
  renameSync(join(root, "fixture-ready.tmp"), join(root, "fixture-ready"));
}
