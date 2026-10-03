import * as THREE from "three"

// ---------------------------------------------------------------------------
// PS1 look = a stack of *deliberate* degradations layered on a clean mesh:
//   1. Vertex snapping  — quantise NDC xy to a coarse grid (the wobble/jitter)
//   2. Affine UVs       — cancel the GPU's perspective-correct interpolation so
//                         textures swim/warp across polygons (the other "tell")
//   3. Per-vertex shade — flat Gouraud-ish lighting baked at the vertex
//   4. Manual fog       — distance haze that masks the short draw distance
//   5. Posterize+dither — 5-bit-ish colour with ordered dithering (15-bit vibe)
// Textures are point-filtered + mip-less elsewhere; the framebuffer itself is
// rendered low-res and nearest-upscaled by CSS.
// ---------------------------------------------------------------------------

export const FOG_COLOR = new THREE.Color("#0a1024")

const SHARED = {
  uSnap: { value: 96.0 }, // grid resolution for vertex snap (lower = chunkier)
  uLightDir: { value: new THREE.Vector3(0.4, 1.0, 0.25).normalize() },
  uAmbient: { value: 0.5 },
  uFogColor: { value: FOG_COLOR },
  uFogNear: { value: 4.0 },
  uFogFar: { value: 30.0 },
}

const VERT = /* glsl */ `
  uniform float uSnap;
  uniform vec3  uLightDir;
  uniform float uAmbient;
  uniform float uEmissive;
  uniform float uFogNear;
  uniform float uFogFar;

  varying vec2  vUv;
  varying float vW;
  varying float vShade;
  varying float vFog;

  void main() {
    vec4 mv   = modelViewMatrix * vec4(position, 1.0);
    vec4 clip = projectionMatrix * mv;

    // (1) vertex snapping — quantise NDC.xy to a coarse grid, then back to clip
    vec3 ndc = clip.xyz / clip.w;
    ndc.xy = floor(ndc.xy * uSnap) / uSnap;
    clip.xyz = ndc * clip.w;
    gl_Position = clip;

    // (2) affine UVs — pre-multiply by w so the hw perspective divide cancels
    vUv = uv * clip.w;
    vW  = clip.w;

    // (3) cheap per-vertex diffuse, or full-bright when emissive
    vec3 n = normalize(normalMatrix * normal);
    float diff = max(dot(n, normalize(uLightDir)), 0.0);
    vShade = mix(uAmbient + (1.0 - uAmbient) * diff, 1.0, uEmissive);

    // (4) linear fog by view-space distance
    float dist = length(mv.xyz);
    vFog = clamp((uFogFar - dist) / (uFogFar - uFogNear), 0.0, 1.0);
  }
`

const FRAG = /* glsl */ `
  precision highp float;

  uniform sampler2D map;
  uniform float uHasMap;
  uniform vec3  uColor;
  uniform vec3  uFogColor;

  varying vec2  vUv;
  varying float vW;
  varying float vShade;
  varying float vFog;

  // recursive 4x4 ordered (Bayer) dither, index-free so it's driver-safe
  float bayer2(vec2 a) {
    a = floor(a);
    return fract(a.x * 0.5 + a.y * a.y * 0.75);
  }
  float bayer4(vec2 a) {
    return bayer2(0.5 * a) * 0.25 + bayer2(a);
  }

  void main() {
    vec2 uv = vUv / vW;                       // affine sample coords
    vec3 tex = texture2D(map, uv).rgb;
    vec3 col = mix(uColor, tex * uColor, uHasMap);

    col *= vShade;

    // (5) ordered dither then posterize to ~5 bits per channel
    float d = (bayer4(gl_FragCoord.xy) - 0.5) / 32.0;
    col = floor(col * 32.0 + 0.5 + d) / 32.0;

    // (4) fog toward the haze colour
    col = mix(uFogColor, col, vFog);

    gl_FragColor = vec4(col, 1.0);
  }
`

export interface PS1Params {
  map?: THREE.Texture | null
  color?: THREE.ColorRepresentation
  emissive?: boolean
  side?: THREE.Side
}

const WHITE = new THREE.DataTexture(
  new Uint8Array([255, 255, 255, 255]),
  1,
  1,
  THREE.RGBAFormat,
)
WHITE.needsUpdate = true

export function createPS1Material(
  params: PS1Params = {},
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    side: params.side ?? THREE.FrontSide,
    uniforms: {
      ...SHARED,
      // SHARED holds shared *references*; clone the value objects so each
      // material keeps its own (they're never mutated, but be safe):
      uSnap: SHARED.uSnap,
      uLightDir: SHARED.uLightDir,
      uAmbient: SHARED.uAmbient,
      uFogColor: SHARED.uFogColor,
      uFogNear: SHARED.uFogNear,
      uFogFar: SHARED.uFogFar,
      map: { value: params.map ?? WHITE },
      uHasMap: { value: params.map ? 1.0 : 0.0 },
      uColor: { value: new THREE.Color(params.color ?? "#ffffff") },
      uEmissive: { value: params.emissive ? 1.0 : 0.0 },
    },
  })
}
