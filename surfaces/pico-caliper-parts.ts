/// <reference path="./pico/caliper/vite-env.d.ts" />
/**
 * Keep this bridge above pico/: Caliper derives surface identity and source-part
 * HMR paths relative to the bridge's directory. The glob includes only Pico;
 * there is no independently maintained list of parts.
 */
export const partsGlob = import.meta.glob<Record<string, unknown>>("./pico/src/**/*.part.tsx")

// Caliper's source-part plugin publishes add/edit/remove events. Accept Vite's
// glob file-set update here so it cannot bubble into a full session reload.
if (import.meta.hot) import.meta.hot.accept()
