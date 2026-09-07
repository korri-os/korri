import { expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react/pure"
import { createInputBus } from "../src/input/bus"
import { createInMemoryOverlayController } from "../src/overlay/in-memory-overlay-controller"
import { OverlayRoot } from "../src/overlay/OverlayRoot"
import { PORTAL_SURFACES } from "../src/surface/surface-registry"

// Import the real registry eagerly. Per-test module aliases or deferred surface
// imports would hide whether the host preload can mount both peer toolchains.
for (const surface of PORTAL_SURFACES) {
  if (!surface.presentations.includes("gameplay-overlay")) continue

  test(`host React renders ${surface.id} from the real surface registry`, async () => {
    const view = render(
      <OverlayRoot
        bus={createInputBus()}
        controller={createInMemoryOverlayController()}
        surface={surface}
      />,
    )
    try {
      expect(
        await view.findByRole("dialog", {
          name: /Browser gameplay fixture/,
        }),
      ).toBeDefined()
      expect(view.getByRole("button", { name: "Open menu" })).toBeDefined()
    } finally {
      cleanup()
    }
  })
}
