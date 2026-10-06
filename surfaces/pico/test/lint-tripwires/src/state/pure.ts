// Tripwire: nothing here may be reported. A parameter that shadows a DOM
// global is not the global.
import type { SurfaceModel } from "@contracts/surface/korri-surface"

export const named = (document: string): number => document.length
export const status = (model: SurfaceModel): string => model.status._tag
