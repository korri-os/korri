import type { KorridClient } from "./client"

/**
 * The kiosk starts beside korrid at boot instead of after it, so the browser
 * is ready by the time the brain is. Before anything mounts, wait until the
 * local korrid answers. Any answer counts, including an error; only
 * "unreachable" keeps waiting. At the deadline the portal mounts anyway and
 * shows what it always showed when korrid was down.
 */
export async function waitForKorrid(
  korrid: Pick<KorridClient, "health">,
  options: {
    readonly intervalMs?: number
    readonly deadlineMs?: number
    readonly now?: () => number
    readonly sleep?: (ms: number) => Promise<void>
  } = {},
): Promise<"answered" | "deadline"> {
  const intervalMs = options.intervalMs ?? 250
  const deadlineMs = options.deadlineMs ?? 120_000
  const now = options.now ?? (() => performance.now())
  const sleep = options.sleep ?? (ms => new Promise<void>(resolve => setTimeout(resolve, ms)))
  const deadline = now() + deadlineMs
  for (;;) {
    const outcome = await korrid.health()
    if (outcome._tag === "Ok" || outcome.payload.code !== "BrainUnreachable") return "answered"
    if (now() + intervalMs >= deadline) return "deadline"
    await sleep(intervalMs)
  }
}
