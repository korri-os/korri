/**
 * The camera at a spot, and where a point in the store lands on screen.
 *
 * One camera, built here, serves both the WebGL scene and the focus targets
 * laid over it, so a target always sits on the tape it stands for. Projection
 * is plain matrix math: it needs no WebGL, so it is tested directly.
 */
import * as THREE from "three"
import type { Spot } from "./boxbuster-spots"
import type { Vec3 } from "./tape-placement"

/** The store's own lens, from legacy. */
const BASE_FOV = 72
/** The narrowest horizontal view a spot may get. A tall container widens the
 * vertical angle until a whole stretch of shelf still fits across. */
const MIN_HORIZONTAL_FOV = 80
export const NEAR = 0.08
export const FAR = 60

const degrees = (radians: number) => (radians * 180) / Math.PI
const radians = (deg: number) => (deg * Math.PI) / 180

/** The vertical field of view for a container of this aspect (width/height). */
export function fovFor(aspect: number): number {
  const keepWide = degrees(
    2 * Math.atan(Math.tan(radians(MIN_HORIZONTAL_FOV / 2)) / aspect),
  )
  return Math.max(BASE_FOV, keepWide)
}

/** Pose a camera at a spot. Works on any perspective camera, so the scene's
 * own camera and a detached one for projection share the same code. */
export function poseCamera(
  camera: THREE.PerspectiveCamera,
  spot: Pick<Spot, "eye" | "yaw" | "pitch">,
  aspect: number,
): void {
  camera.fov = fovFor(aspect)
  camera.aspect = aspect
  camera.near = NEAR
  camera.far = FAR
  camera.position.set(spot.eye.x, spot.eye.y, spot.eye.z)
  camera.rotation.set(spot.pitch, spot.yaw, 0, "YXZ")
  camera.updateProjectionMatrix()
  camera.updateMatrixWorld(true)
}

export interface ScreenPoint {
  readonly x: number
  readonly y: number
  /** False when the point was off screen or behind you and was pulled in to
   * the nearest edge, so it stays reachable. */
  readonly onScreen: boolean
}

/**
 * Where a world point lands in a `width` x `height` container, in CSS pixels.
 * A point off screen is pulled in to the edge it lies past; a point behind
 * you goes to the bottom edge, on the side it lies to. Either way it stays
 * inside `margin` of the edges, so a target for it can always be reached.
 */
export function projectPoint(
  camera: THREE.PerspectiveCamera,
  point: Vec3,
  width: number,
  height: number,
  margin: number,
): ScreenPoint {
  const view = new THREE.Vector3(point.x, point.y, point.z).applyMatrix4(
    camera.matrixWorldInverse,
  )
  const clampX = (x: number) => Math.min(width - margin, Math.max(margin, x))
  const clampY = (y: number) => Math.min(height - margin, Math.max(margin, y))
  if (view.z > -NEAR) {
    const lean = Math.max(-1, Math.min(1, view.x / Math.max(Math.abs(view.z), 1)))
    return {
      x: clampX(width / 2 + lean * (width / 2 - margin)),
      y: height - margin,
      onScreen: false,
    }
  }
  const ndc = view.applyMatrix4(camera.projectionMatrix)
  const x = ((ndc.x + 1) / 2) * width
  const y = ((1 - ndc.y) / 2) * height
  const cx = clampX(x)
  const cy = clampY(y)
  return { x: cx, y: cy, onScreen: cx === x && cy === y }
}
