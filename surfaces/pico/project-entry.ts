/** Tool integration only; never imported by Pico's runtime. */
import "./src/pico.css"
import "./caliper/preview.css"
import { picoAdapter } from "./caliper/adapter"

export const adapters = [picoAdapter]
export { partsGlob } from "../pico-caliper-parts"
export const initialLabPath = "/lab/all/pico/"
