import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { holderEnvironment, startHolder } from "./holder";

const executable = process.env.FEDERATION_FIXTURE_EXECUTABLE;
const project = resolve(import.meta.dir, "../../..");
// The public key of secret 3, the fixture's person signer.
const owner = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";

test.skipIf(!executable)("the holder serves, publishes its owner statement to its relay, and joins on stop", async () => {
  const run = mkdtempSync(join(tmpdir(), "android-app-holder-test-"));
  try {
    const holder = await startHolder(run, project, executable!, "192.0.2.7:47989");
    expect(holder.hostPort).toBeGreaterThan(0);
    expect(holder.relayPort).toBeGreaterThan(0);
    const deadline = Date.now() + 10_000;
    while (holder.events().length === 0 && Date.now() < deadline) await Bun.sleep(100);
    expect(await holder.stop()).toBe(0);
    const events = JSON.parse(readFileSync(join(run, "events.json"), "utf8"));
    expect(events).toHaveLength(1);
    expect(events[0].pubkey).toBe(owner);
    expect(events[0].tags).toContainEqual(["d", `org.korri.device-owner:${holder.hostKey}`]);
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(!executable)("a Moonlight address that cannot be published stops the holder before it is ready", async () => {
  const run = mkdtempSync(join(tmpdir(), "android-app-holder-test-"));
  try {
    await expect(startHolder(run, project, executable!, "192.0.2.7:47989\n")).rejects.toThrow("holder startup failed");
    expect(readFileSync(join(run, "holder.log"), "utf8")).toContain("the Moonlight address cannot be published");
  } finally { rmSync(run, { recursive: true, force: true }); }
}, 30_000);

// #434 D10: the holder provisions the app's viewer certificate through the
// sender's certificate control, and runs as an unprivileged user that has no
// passwd entry, so nix-shell (the systemd fixture's shebang) needs a home.
test("the holder takes its caller's certificate control, input seats and a home of its own, nothing else", () => {
  const run = "/tmp/android-app-holder-run";
  const environment = holderEnvironment(run, project, "/bin", "nixpkgs=/n", {
    KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET: "/run/korri-cadence-certificate-x/control.sock",
    KORRID_STREAM_CERTIFICATE_CONTROL_GID: "59034",
    KORRID_STREAM_CERTIFICATE_CONTROL_PEER_UID: "0",
    KORRID_STREAM_CERTIFICATE_CONTROL_PEER_GID: "0",
    KORRID_INPUT_SEAT_CONTROL_SOCKET: "/run/korri-d10-seats-x/control.sock",
    KORRID_RUNTIME_UID: "0",
    HOME: "/root",
  });
  expect(environment.KORRID_INPUT_SEAT_CONTROL_SOCKET).toBe("/run/korri-d10-seats-x/control.sock");
  expect(environment.KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET).toBe("/run/korri-cadence-certificate-x/control.sock");
  expect(environment.KORRID_STREAM_CERTIFICATE_CONTROL_GID).toBe("59034");
  expect(environment.KORRID_STREAM_CERTIFICATE_CONTROL_PEER_UID).toBe("0");
  expect(environment.KORRID_STREAM_CERTIFICATE_CONTROL_PEER_GID).toBe("0");
  expect(environment.HOME).toBe(join(run, "home"));
  // The caller's other values stay out: the runtime user is the holder's own.
  expect(environment.KORRID_RUNTIME_UID).toBe(String(process.getuid!()));
  expect(holderEnvironment(run, project, "/bin", "", {}).KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET).toBeUndefined();
});
