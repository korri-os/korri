export const name = "hello"
export const systems = {
  hello: { content: { extensions: ["hello"] } },
}
export const runners = {
  hello: { program: "hello", systems: ["hello"] },
}
