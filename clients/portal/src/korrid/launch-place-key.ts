/**
 * One stable string per launch-settings place, shared by the portal's editor
 * mapping and the in-memory brain: `device/any`, `game:wl4/runner:mgba`.
 * Opaque to surfaces; unique within one snapshot.
 */
import type { LaunchLayer, LaunchPlace, LaunchRunnerScope } from "@contracts/generated/korrid"

export function layerKey(layer: LaunchLayer): string {
  switch (layer._tag) {
    case "Device": return "device"
    case "System": return `system:${layer.id}`
    case "Game": return `game:${layer.id}`
    case "Release": return `release:${layer.id.releaseId}`
    case "Profile": return `profile:${layer.id.profileId}`
    case "Launch": return "launch"
  }
}

export function scopeKey(scope: LaunchRunnerScope): string {
  switch (scope._tag) {
    case "Any": return "any"
    case "Family": return `family:${scope.id}`
    case "Runner": return `runner:${scope.id}`
  }
}

export const placeKey = (place: LaunchPlace): string => `${layerKey(place.layer)}/${scopeKey(place.scope)}`
