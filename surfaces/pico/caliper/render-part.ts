import { cloneElement, isValidElement, type ReactElement, type ReactNode } from "react"

interface AuthoredStory {
  readonly render: () => ReactNode
  readonly generatedContract?: { readonly inputs: readonly { readonly id: string }[] }
}

/**
 * Pico's .part functions are pure element factories (checked for every part in
 * caliper.test). Evaluate the authored factory, not the component it returns.
 * Caliper's generated renderer otherwise constructs that component with only
 * editable scalar props, dropping models, arrays, callbacks and children.
 */
export function renderPicoPart(story: AuthoredStory, binding: {
  readonly inputValues: Readonly<Record<string, unknown>>
}): ReactNode {
  const wrapper = story.render()
  if (!story.generatedContract) return wrapper
  if (!isValidElement(wrapper) || typeof wrapper.type !== "function") {
    throw new Error("Pico parts must export a pure element factory")
  }
  const factory = wrapper.type as (props: unknown) => ReactNode
  const root = factory(wrapper.props)
  if (!isValidElement(root)) throw new Error("Pico parts must return one component root")
  const overrides = Object.fromEntries(story.generatedContract.inputs
    .filter(input => Object.hasOwn(binding.inputValues, input.id))
    .map(input => [input.id, binding.inputValues[input.id]]))
  return cloneElement(root as ReactElement<Record<string, unknown>>, overrides)
}
