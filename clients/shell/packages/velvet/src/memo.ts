/**
 * What Velvet remembers between calls, and the one rule for it.
 *
 * `update`, `view` and every selection under them are pure: the same
 * arguments give the same answer. Some answers are kept so they are not
 * worked out again (a view asks for the home's lists several times; Korri
 * publishes with every move of the focus). A kept answer is only ever a
 * faster way to the same answer:
 *
 *   - it is found by everything the answer depends on, exactly (the object
 *     itself, or every value read), never by a part of it or a rounding;
 *   - throwing it away changes nothing but the time the next call takes.
 *
 * Every module that keeps something says here how to forget it. `forget`
 * empties all of them; test/memo.test.ts calls each selection with its
 * memory warm and again after `forget`, and holds the two answers equal.
 * It also holds every module-level binding in src/ that can change to be
 * named in an `onForget` of its file, so nothing is kept outside this rule.
 *
 * State that changes what the person sees is never kept here: it is in
 * Velvet's state (src/types.ts), set by `update`.
 */
const forgetters: (() => void)[] = []

/** Say how to forget what a module keeps. */
export function onForget(forgetIt: () => void): void {
  forgetters.push(forgetIt)
}

/** Forget every kept answer. The next calls work theirs out again, and get the same. */
export function forget(): void {
  for (const forgetIt of forgetters) forgetIt()
}
