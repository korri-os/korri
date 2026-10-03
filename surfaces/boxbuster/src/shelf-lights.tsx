import { useEffect, useLayoutEffect, useMemo, useRef } from "react"
import * as THREE from "three"
import { createPS1Material } from "./ps1-material"
import { SHELF_TUBE_COLOR, type ShelfTube } from "./store-lighting"

/** Both faces of every shelf, in one draw call. The same descriptors supply
 * these physical fixtures and the light spans in the material shader. */
export function ShelfLights({ tubes }: { tubes: readonly ShelfTube[] }) {
  const mesh = useRef<THREE.InstancedMesh>(null)
  const material = useMemo(
    () => createPS1Material({ color: SHELF_TUBE_COLOR, emissive: true }),
    [],
  )
  useEffect(() => () => material.dispose(), [material])
  useLayoutEffect(() => {
    const target = mesh.current
    if (target === null) return
    const transform = new THREE.Matrix4()
    const position = new THREE.Vector3()
    const scale = new THREE.Vector3()
    const rotation = new THREE.Quaternion()
    tubes.forEach((tube, index) => {
      transform.compose(
        position.fromArray(tube.position),
        rotation,
        scale.fromArray(tube.size),
      )
      target.setMatrixAt(index, transform)
    })
    target.instanceMatrix.needsUpdate = true
    target.computeBoundingSphere()
  }, [tubes])
  return (
    <instancedMesh ref={mesh} args={[undefined, material, tubes.length]}>
      <boxGeometry args={[1, 1, 1]} />
    </instancedMesh>
  )
}
