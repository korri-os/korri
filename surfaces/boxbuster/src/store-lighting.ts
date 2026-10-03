/**
 * Option D: warm shelf tubes and blue spill from the CRT. Geometry comes from
 * map.ts; colours, falloff and tube dimensions come from the approved D study.
 * This is renderer data, not configuration or a persisted lighting schema.
 *
 * A store owns its uniform objects. Materials retain references to them, so
 * changing the TV updates the existing materials, not the map or its textures.
 * Two mounted stores never change each other's lighting.
 */
import * as THREE from "three"
import type { TvStatus } from "./boxbuster-store-view"
import { type StoreMap, TV_SCREEN } from "./map"

export const SHELF_TUBE_COLOR = "#ffd38c"
const TUBE_HEIGHT = 0.65
const TUBE_FACE = 0.34
const TUBE_THICKNESS = 0.026
const TUBE_END_GAP = 0.15

export interface ShelfTube {
  readonly position: [number, number, number]
  readonly size: [number, number, number]
}

export interface StoreLighting {
  readonly tubes: readonly ShelfTube[]
  readonly uniforms: {
    readonly uShelfLights: { readonly value: THREE.Vector4[] }
    readonly uTvPosition: { readonly value: THREE.Vector3 }
    readonly uTvSpill: { value: number }
  }
}

export function createStoreLighting(map: StoreMap): StoreLighting {
  const rows = map.gondolas.flatMap(gondola =>
    gondola.levels.map(
      level =>
        new THREE.Vector4(
          gondola.x,
          level + TUBE_HEIGHT,
          gondola.zc,
          gondola.half,
        ),
    ),
  )
  return {
    tubes: rows.flatMap(row =>
      [-1, 1].map(side => ({
        position: [row.x + side * TUBE_FACE, row.y, row.z],
        size: [TUBE_THICKNESS, TUBE_THICKNESS, row.w * 2 - TUBE_END_GAP],
      })),
    ),
    uniforms: {
      uShelfLights: { value: rows },
      uTvPosition: { value: new THREE.Vector3(0, TV_SCREEN.y, TV_SCREEN.z) },
      uTvSpill: { value: 0 },
    },
  }
}

/** Match tv-screen.tsx: work and problems light the screen even without a
 * tape. Otherwise a loaded tape supplies the picture; empty standby is dark.
 * The spill stays blue, as chosen in D, rather than sampling changing artwork. */
export function updateTvSpill(
  lighting: StoreLighting,
  status: TvStatus,
  loaded: boolean,
): void {
  lighting.uniforms.uTvSpill.value =
    loaded || status._tag === "Working" || status._tag === "Problem" ? 1 : 0
}

/** Local light only, before the existing dither and fog. No exposure lift,
 * shadows, bloom, animation, or browser-global mode. Off-shelf fragments skip
 * expensive attenuation work. The cutoff discards less than a dither step.
 * Normals are world-space so turning the camera does not turn these lights. */
export const STORE_LIGHTING_FRAGMENT = /* glsl */ `
  #ifdef STORE_LIGHTING
    uniform vec4 uShelfLights[SHELF_LIGHT_COUNT];
    uniform vec3 uTvPosition;
    uniform float uTvSpill;
    varying vec3 vWorld;
    varying vec3 vWorldNormal;

    vec3 storeIllumination() {
      vec3 illumination = vec3(0.0);
      vec3 n = normalize(vWorldNormal);
      for (int i = 0; i < SHELF_LIGHT_COUNT; i++) {
        vec4 shelf = uShelfLights[i];
        // A span supplies the face on this side of the gondola, never the
        // opposite tube through its backing board.
        vec3 source = vec3(
          shelf.x + (vWorld.x > shelf.x ? ${TUBE_FACE} : -${TUBE_FACE}),
          shelf.y,
          clamp(vWorld.z, shelf.z - shelf.w + 0.1, shelf.z + shelf.w - 0.1)
        );
        vec3 delta = source - vWorld;
        vec3 stretched = delta * vec3(1.4, 1.5, 1.0);
        float d = dot(stretched, stretched);
        if (d > 8.0) continue;
        vec3 direction = delta / max(length(delta), 0.0001);
        float facing = 0.25 + 0.75 * max(0.0, dot(n, direction));
        illumination += vec3(1.0, 0.66, 0.3) * exp(-d * 1.6) * facing * 0.9;
      }
      if (uTvSpill > 0.0) {
        vec3 delta = uTvPosition - vWorld;
        float distanceSquared = dot(delta, delta);
        if (distanceSquared < 208.0) {
          vec3 direction = delta / max(length(delta), 0.0001);
          float facing = 0.35 + 0.65 * max(0.0, dot(n, direction));
          illumination += vec3(0.19, 0.52, 1.0)
            * exp(-distanceSquared / 26.0) * facing * 1.25;
        }
      }
      return illumination;
    }
  #endif
`
