/**
 * The store itself: one WebGL canvas, rendered low-res and upscaled
 * nearest-neighbour so it reads as a PS1 framebuffer.
 *
 * The camera stands still at the visit's spot, posed by the same code the
 * focus targets are projected with (boxbuster-camera.ts), so a target always
 * sits on the tape it stands for. Moving between spots is a cut: every device
 * runs with reduced motion, and a still camera keeps the targets still.
 *
 * This is the one opaque leaf. It is reached through BoxbusterStoreDrawing,
 * so the rest of the surface runs and is tested where WebGL is missing.
 */
import { Canvas, useThree } from "@react-three/fiber"
import { type ComponentType, useLayoutEffect } from "react"
import * as THREE from "three"
import { FAR, NEAR, poseCamera } from "./boxbuster-camera"
import type { Spot } from "./boxbuster-spots"
import { canDrawStore } from "./boxbuster-webgl"
import type { StoreGame, StoreMap } from "./map"
import { FOG_COLOR } from "./ps1-material"
import { Scene } from "./scene"
import type { PlacedTape } from "./tape-placement"

export interface BoxbusterStoreProps {
  readonly map: StoreMap
  readonly placed: readonly PlacedTape[]
  readonly spot: Spot
  readonly held?: { readonly tapeId: string; readonly face: "front" | "back" }
  readonly inDeck?: StoreGame
  readonly deckLabels?: readonly string[]
}

/** How the store is drawn, and whether it can be here at all. */
export interface BoxbusterStoreDrawing {
  readonly available: () => boolean
  readonly Draw: ComponentType<BoxbusterStoreProps>
}

export function BoxbusterStore({ spot, ...scene }: BoxbusterStoreProps) {
  return (
    <Canvas
      flat
      dpr={0.32} // low-res framebuffer; CSS does the nearest-neighbour upscale
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
