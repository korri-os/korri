// Probe composition only: always use the pinned source implementation.
// Public entry point plus real codecs needed at @exodus/bytes' module scope
// (whatwg-url's browser graph). Other legacy encoding tables stay excluded.
import { TextEncoder, TextDecoder } from "@kayahr/text-encoding/no-encodings"
import "@kayahr/text-encoding/encodings/windows-1252"
import "@kayahr/text-encoding/encodings/utf-16le"
import "@kayahr/text-encoding/encodings/utf-16be"
Object.assign(globalThis, { TextEncoder, TextDecoder })
