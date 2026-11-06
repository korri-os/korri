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
 * so unchanged parts keep their identity: memos keyed on them hit.
 */
export function share<T>(prev: unknown, next: T): T {
  if (prev === next || next === null || typeof next !== "object" || prev === null || typeof prev !== "object") return next
  // Nothing is copied until a part is kept from neither side: a model equal
  // to the last one, parsed afresh, allocates nothing here.
  if (Array.isArray(next)) {
    if (!Array.isArray(prev)) return next
    const length = next.length
    let same = prev.length === length
    let own = true
    let out: unknown[] | undefined
    for (let i = 0; i < length; i += 1) {
      const item: unknown = next[i]
      const kept = share(prev[i], item)
      if (out !== undefined) {
        out.push(kept)
        continue
      }
      const wasSame = same
      if (kept !== prev[i]) same = false
      if (kept !== item) own = false
      if (!same && !own) {
        // Every part before this one was kept from one side: copy that side.
        out = (wasSame ? prev : next).slice(0, i) as unknown[]
        out.push(kept)
      }
    }
    return (out ?? (same ? prev : next)) as T
  }
  if (Array.isArray(prev)) return next
  const before = prev as Record<string, unknown>
  const after = next as Record<string, unknown>
  let same = true
  let own = true
  let out: Record<string, unknown> | undefined
  let keys = 0
  for (const key in after) {
    keys += 1
    const item = after[key]
    const kept = share(before[key], item)
    if (out !== undefined) {
      out[key] = kept
      continue
    }
    const wasSame = same
    if (kept !== before[key] || (kept === undefined && !(key in before))) same = false
    if (kept !== item) own = false
    if (!same && !own) {
      // Every part before this one was kept from one side: copy that side.
      const from = wasSame ? before : after
      out = {}
      for (const earlier in after) {
        if (earlier === key) break
        out[earlier] = from[earlier]
      }
      out[key] = kept
    }
  }
  if (out !== undefined) return out as T
  if (same) {
    for (const _ in before) keys -= 1
    if (keys === 0) return prev as T
  }
  // Wholly equal: the old object. Nothing reused: the new object itself,
  // so what was built from it stays keyed to it.
  return next
}
