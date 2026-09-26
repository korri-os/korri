// Development only: `bun run caliper` shows Pico's parts at true device size.
// The portal builds Pico from source with its own Vite config, not this one.
import { caliper } from "@simonwjackson/caliper"
import { defineConfig } from "vite"

export default defineConfig({ plugins: [caliper()] })
