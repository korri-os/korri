package com.limelight.computers;

/** Keeps slow Moonlight network reads outside the per-host mutation lock. */
final class KorriMoonlightNetworkCycle {
    interface Fetch<T> {
        T run() throws Exception;
    }

    interface Commit<T, R> {
        R run(T value) throws Exception;
    }

    private KorriMoonlightNetworkCycle() {}

    static <T, R> R fetchThenCommit(
            Object mutationLock, Fetch<T> fetch, Commit<T, R> commit) throws Exception {
        T value = fetch.run();
        synchronized (mutationLock) {
            return commit.run(value);
        }
    }
}
