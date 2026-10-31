/**
 * Preview only: prints Pico's own fixtureModel (the model Caliper's Home Ready
 * state draws) as JSON, so main.slint can be drawn with the reference's data.
 *
 *   bun preview/pico-fixture-model.ts > preview/gen/pico-fixture-model.json
 */
import { fixtureModel } from "../../../../../surfaces/pico/src/fixtures/fixture-host"

console.log(JSON.stringify(fixtureModel, null, 2))
