/**
 * The store itself: one WebGL canvas, rendered low-res and upscaled
 * nearest-neighbour so it reads as a PS1 framebuffer.
 *
 * The camera stands still at the visit's spot, posed by the same code the
 * focus targets are projected with (boxbuster-camera.ts), so a target always
 * sits on the thing it stands for. Moving between spots is a cut: every
 * device runs with reduced motion, and a still camera keeps targets still.
 *
 * Everything you see is in the room: what has focus, the box you read, and
 * Korri's words on the TV. Nothing is laid over it. This is the one opaque
 * leaf, reached through BoxbusterStoreDrawing, so the rest of the surface
 * runs and is tested where WebGL is missing.
 */
import { Canvas, useThree } from "@react-three/fiber"
import { type ComponentType, useLayoutEffect } from "react"
import * as THREE from "three"
import { FAR, NEAR, poseCamera } from "./boxbuster-camera"
import type { Spot } from "./boxbuster-spots"
import type { TvStatus } from "./boxbuster-store-view"
import type { Target } from "./boxbuster-targets"
import type { Hand } from "./boxbuster-visit"
import { canDrawStore } from "./boxbuster-webgl"
import type { StoreGame, StoreMap } from "./map"
import { FOG_COLOR } from "./ps1-material"
import { Scene } from "./scene"
import type { PlacedTape } from "./tape-placement"

export type HeldTape = Omit<Extract<Hand, { _tag: "Holding" }>, "_tag">

export interface BoxbusterStoreProps {
  readonly map: StoreMap
  readonly placed: readonly PlacedTape[]
  readonly spot: Spot
  /** The container, in CSS pixels. */
  readonly width: number
  readonly height: number
  /** The unseen focus targets, and which one has focus. */
  readonly targets: readonly Target[]
  readonly focused?: string
  readonly held?: HeldTape
  readonly inDeck?: StoreGame
  /** One sign per deck when Korri offers several places to play. */
  readonly deckLabels?: readonly string[]
  /** What the TV says about a launch. */
  readonly tv: TvStatus
}

/** How the store is drawn, and whether it can be here at all. */
export interface BoxbusterStoreDrawing {
  readonly available: () => boolean
  readonly Draw: ComponentType<BoxbusterStoreProps>
}

/** The store's own pixel ratio: a PS1-coarse 0.32, but never a framebuffer
 * smaller than a PS1's 320x240, below which a box's title is mush. */
const BASE_DPR = 0.32
const MIN_FRAME = { width: 320, height: 240 }

export function storeDpr(width: number, height: number): number {
  if (width <= 0 || height <= 0) return BASE_DPR
  const floor = Math.max(MIN_FRAME.width / width, MIN_FRAME.height / height)
  const ceiling =
    typeof window === "undefined" ? 1 : (window.devicePixelRatio ?? 1)
  return Math.min(ceiling, Math.max(BASE_DPR, floor))
}

export function BoxbusterStore({ spot, width, height, ...scene }: BoxbusterStoreProps) {
  return (
    <Canvas
      flat
      dpr={storeDpr(width, height)} // CSS does the nearest-neighbour upscale
      gl={{ antialias: false, powerPreference: "high-performance" }}
      camera={{ near: NEAR, far: FAR }}
      onCreated={({ scene: world, gl }) => {
        world.background = FOG_COLOR
        gl.setClearColor(FOG_COLOR)
        gl.outputColorSpace = THREE.SRGBColorSpace
      }}
    >
      <BoxbusterStoreCamera spot={spot} />
      <Scene {...scene} />
    </Canvas>
  )
}

function BoxbusterStoreCamera({ spot }: { spot: Spot }) {
  const camera = useThree(state => state.camera)
  const width = useThree(state => state.size.width)
  const height = useThree(state => state.size.height)
  useLayoutEffect(() => {
    if (!(camera instanceof THREE.PerspectiveCamera) || height === 0) return
    poseCamera(camera, spot, width / height)
  }, [camera, spot, width, height])
  return null
}

export const webglStore: BoxbusterStoreDrawing = {
  available: canDrawStore,
  Draw: BoxbusterStore,
}
