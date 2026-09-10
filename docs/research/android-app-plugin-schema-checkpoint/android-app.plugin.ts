// Declaration-only Android application launcher plugin for Checkpoint 0.
//
// `android-app` is the integration token already consumed by Korri's signed
// Android LaunchSpec. It is not a process command: the future Android launch
// integration must consume it before generic process execution.

export const name = "android-app"
export const title = "Android"
export const providers = {
  "@korri:android-app": {
    id: "@korri:android-app",
    title: "Android",
  },
}
export const systems = {
  android: {
    id: "android",
    title: "Android",
  },
}
export const launchers = {
  "android-app": {
    id: "@korri:android-app/android-app",
    plugin: "@korri:android-app",
    command: "android-app",
    systems: ["android"],
  },
}
