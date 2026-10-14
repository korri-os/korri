import { PicoSetupWelcome } from "./PicoSetupWelcome"

export const name = "Setup Welcome"
export const note = "Hello, what setup will ask for, and START"

export default function PicoSetupWelcomePart() {
  return <PicoSetupWelcome askWifi onStart={() => undefined} />
}

export function NoWifiToAsk() {
  return <PicoSetupWelcome askWifi={false} onStart={() => undefined} />
}
