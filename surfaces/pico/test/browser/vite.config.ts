// The server test/pico-displays-browser.test.ts starts for its Chromium page:
// Pico's source and nothing else, without Caliper.
import { defineConfig } from "vite"

export default defineConfig({ logLevel: "info", server: { host: "127.0.0.1" } })
