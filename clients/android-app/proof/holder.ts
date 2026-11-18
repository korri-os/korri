// The holder of the Android app's holder proof (#434 D9), on this host: the
// federation fixture's real korrid host (services/korrid/examples/
// federation_fixture.rs) and its NIP-01 relay (nostr-relay-fixture.ts, from
// the Artemis app's federation acceptance at the tag `artemis-final`). The
// holder's game streams from the Sunshine at MOONLIGHT.
//
//   bun holder.ts RUN PROJECT EXECUTABLE MOONLIGHT
//
// RUN/ready gets "hostPort hostKey relayPort" when the holder serves. On
// SIGTERM or SIGINT the relay's events go to RUN/events.json, the holder
// stops and joins its discovery, and the relay stops. Test-process only.
//
// For the D10 proof the holder launches the game and provisions the app's
// viewer certificate: its caller names the sender's certificate control
// (KORRID_STREAM_CERTIFICATE_CONTROL_*, services/korrid/src/host/
// moonlight_certificate.rs) and the proof's input-seat receiver
// (KORRID_INPUT_SEAT_CONTROL_SOCKET, host/mod.rs; proof/seats.py), and runs
// this as an unprivileged user.
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import type { Subprocess } from "bun";
import { hostEnvironment, startRelay } from "./nostr-relay-fixture";

// What korrid reads of its caller's sockets, when the caller names them: the
// sender's certificate control, and the input-seat receiver.
const PASSED = (name: string) => name.startsWith("KORRID_STREAM_CERTIFICATE_CONTROL_") || name === "KORRID_INPUT_SEAT_CONTROL_SOCKET";

/** The acceptance's isolated environment, the caller's certificate control
 * and input-seat receiver, and a home inside the run directory: nix-shell,
 * the systemd fixture's shebang, needs one for a user without a passwd entry.
 * No other caller value reaches the holder. */
export function holderEnvironment(run: string, project: string, path: string, nixPath: string, caller: Record<string, string | undefined>) {
  const passed = Object.fromEntries(Object.entries(caller).filter((entry): entry is [string, string] =>
    PASSED(entry[0]) && entry[1] !== undefined));
  return { ...hostEnvironment(run, project, path, nixPath), ...passed, HOME: join(run, "home") };
}

export async function startHolder(run: string, project: string, executable: string, moonlight: string) {
  if (![run, project, executable].every(isAbsolute) || existsSync(join(run, "host"))) throw new Error("fresh absolute run directory required");
  mkdirSync(join(run, "units"), { recursive: true });
  mkdirSync(join(run, "home"), { recursive: true, mode: 0o700 });
  const relay = startRelay();
  const log = openSync(join(run, "holder.log"), "a", 0o600);
  let holder: Subprocess;
  try {
    // The same isolated environment as the acceptance flow's host: no caller
    // KORRID_* value, HOME or XDG state reaches it.
    holder = Bun.spawn([executable, join(run, "host"), "0", JSON.stringify([relay.url]), moonlight], {
      env: holderEnvironment(run, project, process.env.PATH ?? "", process.env.NIX_PATH ?? "", process.env), stdout: log, stderr: log,
    });
  } catch (error) { closeSync(log); await relay.stop(); throw error; }
  closeSync(log);
  async function stop(): Promise<number> {
    writeFileSync(join(run, "events.json"), JSON.stringify(relay.events(), null, 1));
    holder.kill("SIGTERM");
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([holder.exited, new Promise<number>((resolve) => {
        timer = setTimeout(() => { holder.kill("SIGKILL"); resolve(-1); }, 10_000);
      })]);
    } finally { clearTimeout(timer); await relay.stop(); }
  }
  const deadline = Date.now() + 30_000;
  while (!existsSync(join(run, "host/ready"))) {
    if (holder.exitCode !== null || Date.now() >= deadline) { await stop(); throw new Error("holder startup failed or timed out; read holder.log"); }
    await Bun.sleep(50);
  }
  const [port, key] = readFileSync(join(run, "host/ready"), "utf8").trim().split(" ");
  if (!/^\d+$/.test(port!) || !/^[a-f0-9]{64}$/.test(key!)) { await stop(); throw new Error("holder ready line is malformed"); }
  return { hostPort: Number(port), hostKey: key!, relayPort: relay.port, events: relay.events, stop };
}

if (import.meta.main) {
  const [run, project, executable, moonlight] = process.argv.slice(2);
  if (!run || !project || !executable || !moonlight || process.argv.length !== 6) throw new Error("run project executable moonlight required");
  const holder = await startHolder(run, project, executable, moonlight);
  let stopping = false;
  const stop = async () => {
    if (stopping) return;
    stopping = true;
    const code = await holder.stop();
    console.log(code === 0 ? "holder stopped with joined discovery" : `holder exit ${code}`);
    process.exit(code === 0 ? 0 : 1);
  };
  process.on("SIGTERM", stop); process.on("SIGINT", stop);
  writeFileSync(join(run, "ready.tmp"), `${holder.hostPort} ${holder.hostKey} ${holder.relayPort}\n`);
  renameSync(join(run, "ready.tmp"), join(run, "ready"));
  console.log(`holder ready port=${holder.hostPort} device=${holder.hostKey} relay=${holder.relayPort}`);
}
