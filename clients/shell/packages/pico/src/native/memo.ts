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
