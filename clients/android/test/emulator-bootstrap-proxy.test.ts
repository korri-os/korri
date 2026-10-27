import { expect, test } from "bun:test";
import { createConnection, createServer } from "node:net";
import { startBootstrapProxy } from "./emulator-bootstrap-proxy";

async function request(port: number, data: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: "127.0.0.1", port });
    let result = "";
    socket.setTimeout(3_000, () => { socket.destroy(); reject(new Error("TCP response timed out")); });
    socket.on("error", reject);
    socket.on("connect", () => socket.write(data));
    socket.on("data", (data) => { result += data.toString(); });
    socket.on("end", () => resolve(result));
  });
}

test("bootstrap proxy denies CONNECT to a real reachable target and never forwards traffic", async () => {
  let connections = 0;
  const target = createServer((socket) => { connections++; socket.end("unexpected forwarding"); });
  await new Promise<void>((resolve) => target.listen(0, "127.0.0.1", resolve));
  const address = target.address();
  if (!address || typeof address === "string") throw new Error("TCP required");
  const denied: string[] = [];
  const proxy = await startBootstrapProxy((target) => denied.push(target));
  try {
    expect(await request(address.port, "probe")).toBe("unexpected forwarding");
    const destination = `127.0.0.1:${address.port}`;
    expect(await request(proxy.port, `CONNECT ${destination} HTTP/1.1\r\nAuthorization: secret\r\n\r\n`)).toStartWith("HTTP/1.1 403 Forbidden\r\n");
    expect(await request(proxy.port, `GET http://${destination}/ HTTP/1.1\r\n\r\n`)).toStartWith("HTTP/1.1 403 Forbidden\r\n");
    expect(await request(proxy.port, "arbitrary TCP payload")).toStartWith("HTTP/1.1 403 Forbidden\r\n");
    expect(denied).toEqual([destination]);
    expect(connections).toBe(1);
  } finally {
    await proxy.stop();
    await new Promise<void>((resolve, reject) => target.close((error) => error ? reject(error) : resolve()));
  }
  await expect(request(proxy.port, "after close")).rejects.toMatchObject({ code: "ECONNREFUSED" });
});
