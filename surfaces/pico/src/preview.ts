/**
 * Pico's development surface.
 *
 * A design tool needs two things Pico's runtime entry deliberately does not
 * expose: a model with representative data in it, and a host that answers
 * without Korri running. They are the same fixtures the tests use, so what a
 * tool shows and what the suite asserts cannot drift apart.
 *
 * This is an entrypoint, not a barrel — it exists to define exactly what a
 * development consumer may reach for, which is why it re-exports two values and
 * nothing else.
 */
export { createFixtureHost, fixtureModel } from "./fixtures/fixture-host"
export type { FixtureHost } from "./fixtures/fixture-host"
