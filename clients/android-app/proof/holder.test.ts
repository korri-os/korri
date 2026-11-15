import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { startHolder } from "./holder";

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
