// Identical inputs/code run with native Bun globals and with source libraries in
// QuickJS. These are focused differential cases, not the full WHATWG/WPT suites.
export function primitiveCases(): Record<string, unknown> {
  const results: Record<string, unknown> = {}
  const check = (name: string, run: () => unknown) => {
    try { results[name] = { ok: true, value: run() } }
    catch (error) { results[name] = { ok: false, error: error instanceof Error ? error.name : String(error) } }
  }
  for (const [name, text] of [
    ["ascii", "Hello"], ["utf8", "é中文"], ["non-bmp", "A😀𝄞Z"],
    ["lone-surrogates", "a\ud800b\udcffc"], ["bom", "\ufeffhello"],
  ]) {
    check(`encode-${name}`, () => Array.from(new TextEncoder().encode(text)))
    check(`roundtrip-${name}`, () => new TextDecoder().decode(new TextEncoder().encode(text)))
  }
  for (const [name, bytes] of [
    ["overlong", [0xc0, 0xaf]], ["truncated", [0xf0, 0x9f, 0x98]],
    ["surrogate", [0xed, 0xa0, 0x80]], ["out-of-range", [0xf4, 0x90, 0x80, 0x80]],
    ["bad-continuation", [0xe2, 0x28, 0xa1]], ["bom", [0xef, 0xbb, 0xbf, 65]],
  ] as const) {
    for (const fatal of [false, true]) {
      check(`decode-${name}-fatal-${fatal}`, () => new TextDecoder("utf-8", { fatal }).decode(new Uint8Array(bytes)))
    }
  }
  check("decode-ignore-bom", () => new TextDecoder("utf-8", { ignoreBOM: true }).decode(new Uint8Array([0xef, 0xbb, 0xbf, 65])))
  check("decode-stream-non-bmp", () => {
    const decoder = new TextDecoder()
    return [decoder.decode(new Uint8Array([0xf0, 0x9f]), { stream: true }), decoder.decode(new Uint8Array([0x98, 0x80]))]
  })
  check("decode-stream-bom", () => {
    const decoder = new TextDecoder()
    return [decoder.decode(new Uint8Array([0xef]), { stream: true }), decoder.decode(new Uint8Array([0xbb, 0xbf, 65]))]
  })
  check("decode-view-offset", () => new TextDecoder().decode(new DataView(new Uint8Array([0, 65, 66, 0]).buffer, 1, 2)))
  check("decode-reuse-after-fatal", () => {
    const decoder = new TextDecoder("utf-8", { fatal: true })
    try { decoder.decode(new Uint8Array([0xff])) } catch { /* Test recovery on the same decoder. */ }
    return decoder.decode(new Uint8Array([65]))
  })
  check("encoder-encodeInto", () => {
    const encoder = new TextEncoder()
    return [3, 4, 5, 6].map(size => {
      const output = new Uint8Array(size)
      return { ...encoder.encodeInto("A😀Z", output), bytes: Array.from(output) }
    })
  })
  // These deliberately expose WebIDL/API gaps, rather than hiding them behind adapters.
  check("encoder-string-coercion", () => new TextEncoder().encode(123 as never).join(","))
  check("decoder-invalid-input", () => new TextDecoder().decode(123 as never))
  check("decoder-legacy-label", () => new TextDecoder("windows-1252").decode(new Uint8Array([0x80])))
  const urls: [string, string, string?][] = [
    ["canonical", "HTTPS://EXAMPLE.COM:443/a/../b?q=hello world#é"],
    ["idna", "https://測試.example/😀"],
    ["idna-sharp-s", "https://faß.de/"],
    ["ipv4", "https://0x7f.1/"],
    ["ipv6", "https://[2001:0db8:0:0:0:0:0:1]:443/a"],
    ["credentials", "https://a:b@example.com:8443/"],
    ["base", "../c?x=1", "https://example.com/a/b"],
    ["encoded-dot", "https://example.com/a/%2e%2e/b"],
    ["backslash", "https:\\example.com\\a"],
    ["lone-surrogate", "https://example.com/\ud800"],
    ["opaque", "mailto:a@example.com"],
    ["file", "file:///tmp/../example"],
    ["invalid-host", "https://"],
    ["invalid-port", "https://example.com:65536/"],
    ["invalid-percent-host", "https://%zz/"],
    ["invalid-ipv6", "https://[:::]/"],
    ["relative-no-base", "/relative"],
  ]
  for (const [name, input, base] of urls) {
    check(`url-${name}`, () => {
      const url = new URL(input, base)
      return { href: url.href, origin: url.origin, protocol: url.protocol, host: url.host,
        pathname: url.pathname, search: url.search, hash: url.hash, params: [...url.searchParams] }
    })
  }
  check("search-params", () => {
    const params = new URLSearchParams("b=2&a=one+two&a=%F0%9F%98%80&bad=%FF&bom=%EF%BB%BF")
    params.append("surrogate", "\ud800")
    params.sort()
    return { entries: [...params], encoded: params.toString(), all: params.getAll("a") }
  })
  check("url-search-params-live", () => {
    const url = new URL("https://example.com/?a=hello%20world")
    const params = url.searchParams
    params.append("x", "~")
    const first = url.href
    url.search = "?b=2"
    return { first, entries: [...params], same: params === url.searchParams, href: url.href }
  })
  check("url-statics", () => ({ valid: URL.canParse("https://example.com"), invalid: URL.canParse("https://"), parsed: URL.parse("https://EXAMPLE.COM:443/")?.href }))
  return results
}
