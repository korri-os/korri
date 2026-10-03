/**
 * The store itself: one WebGL canvas, rendered low-res and upscaled
 * nearest-neighbour so it reads as a PS1 framebuffer.
 *
 * The camera stands just inside the entrance, looking in. Moving it is the
 * focus-driven input model's job, which is not built yet; legacy's WASD and
 * mouse-look stayed on `legacy` on purpose.
 */
import { Canvas, useThree } from "@react-three/fiber"
import { useEffect } from "react"
import * as THREE from "three"
import type { BoxbusterStoreView } from "./boxbuster-store-view"
import { FOG_COLOR } from "./ps1-material"
import { Scene } from "./scene"

/** Eye height, from legacy's first-person controls. */
const EYE = 1.55

export function BoxbusterStore({ view }: { view: BoxbusterStoreView }) {
  if (view._tag !== "Open") return null
  const { map } = view
  return (
    <Canvas
      flat
      dpr={0.32} // low-res framebuffer; CSS does the nearest-neighbour upscale
      gl={{ antialias: false, powerPreference: "high-performance" }}
      camera={{ fov: 72, near: 0.08, far: 60 }}
      onCreated={({ scene, gl }) => {
        scene.background = FOG_COLOR
        gl.setClearColor(FOG_COLOR)
        gl.outputColorSpace = THREE.SRGBColorSpace
      }}
    >
      <BoxbusterStoreCamera x={map.camStart.x} z={map.camStart.z} />
      <Scene map={map} />
    </Canvas>
  )
}

/** Puts the camera at the entrance whenever the entrance moves (a bigger
 * library builds a deeper New Releases room). */
function BoxbusterStoreCamera({ x, z }: { x: number; z: number }) {
  const camera = useThree(state => state.camera)
  useEffect(() => {
    camera.position.set(x, EYE, z)
    camera.rotation.set(0, 0, 0) // looking north, into the store
  }, [camera, x, z])
  return null
}
