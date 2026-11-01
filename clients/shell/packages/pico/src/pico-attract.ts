/**
 * How long the library sits untouched before Pico starts showing off.
 *
 * Legacy had no idle policy at all — attract was a screen in a gallery — so
 * this is a decision rather than a port, and the number is the whole decision.
 *
 * Forty-five seconds. Shorter than that and it interrupts: reading a shelf
 * caption, deciding between two games, or answering a launch-location question
 * all take longer than half a minute, and a screen that changes under a
 * thinking user is a screen they stop trusting. Much longer and nobody ever
 * sees it — a handheld put down on a table is usually picked up again or
 * blanked by the operating system within a couple of minutes.
 */
export const PICO_ATTRACT_AFTER_MS = 45_000
