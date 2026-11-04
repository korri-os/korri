/**
 * Remember the last few results of a pure function, by the identity of its
 * inputs. The shell runs logic.js in QuickJS, an interpreter: building the
 * same screen four times in one press costs milliseconds on a handheld.
 * Every input Pico passes is immutable, so the same objects mean the same
 * result.
 */
export function memo<Args extends readonly unknown[], Result>(size: number, keyOf: (...args: Args) => readonly unknown[], compute: (...args: Args) => Result): (...args: Args) => Result {
  const entries: { readonly key: readonly unknown[]; readonly result: Result }[] = []
  return (...args: Args): Result => {
    const key = keyOf(...args)
    for (let i = 0; i < entries.length; i += 1) {
      const entry = entries[i]
      if (entry !== undefined && entry.key.length === key.length && entry.key.every((part, j) => part === key[j])) {
        if (i > 0) {
          entries.splice(i, 1)
          entries.unshift(entry)
        }
        return entry.result
      }
    }
    const result = compute(...args)
    entries.unshift({ key, result })
    if (entries.length > size) entries.pop()
    return result
  }
}

/**
 * `next` with every part equal to `prev`'s replaced by `prev`'s own object,
 * so unchanged parts keep their identity: memos keyed on them hit, and the
 * shell keeps the models of arrays that did not change.
 */
export function share<T>(prev: unknown, next: T): T {
  if (prev === next) return next
  if (Array.isArray(next)) {
    if (!Array.isArray(prev) || prev.length !== next.length) return next.map((item, i) => share(Array.isArray(prev) ? prev[i] : undefined, item)) as T
    let same = true
    let own = true
    const out = next.map((item, i) => {
      const kept = share(prev[i], item)
      if (kept !== prev[i]) same = false
      if (kept !== item) own = false
      return kept
    })
    return (same ? prev : own ? next : out) as T
  }
  if (next !== null && typeof next === "object") {
    if (prev === null || typeof prev !== "object" || Array.isArray(prev)) return next
    const before = prev as Record<string, unknown>
    const keys = Object.keys(next)
    let same = keys.length === Object.keys(before).length
    let own = true
    const out: Record<string, unknown> = {}
    for (const key of keys) {
      const item = (next as Record<string, unknown>)[key]
      const kept = share(before[key], item)
      if (kept !== before[key]) same = false
      if (kept !== item) own = false
      out[key] = kept
    }
    // Wholly equal: the old object. Nothing reused: the new object itself,
    // so what was built from it stays keyed to it.
    return (same ? prev : own ? next : out) as T
  }
  return next
}
