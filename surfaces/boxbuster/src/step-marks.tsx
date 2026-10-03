/**
 * Worn patches in the carpet where you can step next.
 *
 * Every way on from where you stand is a patch on the floor, under its
 * unseen focus target: the scene casts the target's screen point down onto
 * the carpet, so a way on behind you shows as a patch at your feet. The
 * patches are always there, faintly, like carpet worn by other customers;
 * the one with focus glows.
 */
import { useFrame, useThree } from "@react-three/fiber"
import { useMemo, useRef } from "react"
import * as THREE from "three"
import { FLOOR_MARK } from "./boxbuster-spots"
import type { Target } from "./boxbuster-targets"
import { createPS1Material } from "./ps1-material"

const RADIUS = 0.45

type ExitTarget = Extract<Target, { _tag: "Exit" }>

export function StepMarks({
  exits,
  focused,
}: {
  exits: readonly ExitTarget[]
  focused: string | undefined
}) {
  const mats = useMemo(
    () => ({
      // unshaded, so a worn patch reads the same in a dim aisle as in the lobby
      worn: createPS1Material({ color: "#3a5486", emissive: true }),
      lit: createPS1Material({ color: "#f2c100", emissive: true }),
    }),
    [],
  )
  const marks = useRef<(THREE.Mesh | null)[]>([])
  const size = useThree(state => state.size)
  const helpers = useMemo(
    () => ({
      ray: new THREE.Raycaster(),
      floor: new THREE.Plane(new THREE.Vector3(0, 1, 0), -FLOOR_MARK),
      hit: new THREE.Vector3(),
      ndc: new THREE.Vector2(),
    }),
    [],
  )

  // The camera is posed in a layout effect; place the marks once it is.
  useFrame(({ camera }) => {
    exits.forEach((exit, i) => {
      const mark = marks.current[i]
      if (!mark || size.width === 0) return
      helpers.ndc.set(
        (exit.x / size.width) * 2 - 1,
        1 - (exit.y / size.height) * 2,
      )
      helpers.ray.setFromCamera(helpers.ndc, camera)
      const onFloor = helpers.ray.ray.intersectPlane(helpers.floor, helpers.hit)
      mark.visible = onFloor !== null
      if (onFloor !== null) mark.position.copy(helpers.hit)
    })
  })

  return (
    <group>
      {exits.map((exit, i) => (
        <mesh
          key={exit.key}
          ref={el => {
            marks.current[i] = el
          }}
          rotation={[-Math.PI / 2, 0, 0]}
          material={exit.key === focused ? mats.lit : mats.worn}
        >
          <circleGeometry args={[RADIUS, 10]} />
        </mesh>
      ))}
    </group>
  )
}
