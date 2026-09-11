// Evaluated AFTER text-platform in QuickJS, never alongside its static imports.
// This is the real WHATWG parser, including WebIDL and UTS #46 processing.
import { URL, URLSearchParams } from "whatwg-url"
Object.assign(globalThis, { URL, URLSearchParams })
