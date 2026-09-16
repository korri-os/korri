export const name = "android-app"
export const title = "Android"
export const providers = {
  "@korri:android-app": { id: "@korri:android-app", title: "Android" },
}
export const systems = {
  android: { id: "android", title: "Android" },
}
export const runners = {
  "android-app": {
    id: "@korri:android-app/android-app",
    family: "@korri:android-app",
    command: "android-app",
    systems: ["android"],
  },
}
