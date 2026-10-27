// Emulator transport fixture: deny bootstrap TCP before the guest starts.
// No forwarding, DNS lookup, or upstream socket exists in this implementation.
import { writeFileSync } from "node:fs";
import { createServer } from "node:net";

export async function startBootstrapProxy(onDenied: (target: string) => void) {
  const server = createServer({ allowHalfOpen: true }, (socket) => {
    socket.setTimeout(2_000, () => socket.destroy());
    socket.on("error", () => socket.destroy());
    socket.once("data", (data) => {
      // Metadata only. Never log headers, credentials, or request bodies.
      const target = /^CONNECT ([a-zA-Z0-9.:[\]-]+) HTTP\/1\.[01]\r\n/.exec(data.toString())?.[1];
      if (target) onDenied(target);
      socket.end("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("TCP listener required");
  return { port: address.port, stop: () => new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve())) };
}

if (import.meta.main) {
  const [ready] = process.argv.slice(2);
  if (!ready || process.argv.length !== 3) throw new Error("ready path required");
  const proxy = await startBootstrapProxy((target) => console.log(`denied ${target}`));
  process.on("SIGTERM", () => { void proxy.stop().then(() => process.exit(0)); });
  writeFileSync(ready, `${proxy.port}\n`);
}
