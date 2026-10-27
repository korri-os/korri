package com.limelight;

/** Exact token and short lease for ordering asynchronous Moonlight connection callbacks. */
final class KorriConnectionLifecycle {
    private long token;

    synchronized long started() {
        return ++token;
    }

    synchronized void retire() {
        token++;
    }

    synchronized boolean isCurrent(long expectedToken) {
        return token == expectedToken;
    }

    /**
     * Runs short, nonblocking start work while holding the exact lifecycle lease.
     * A concurrent retire waits until the work completes, then makes the token stale.
     */
    synchronized boolean runIfCurrent(long expectedToken, Runnable action) {
        if (token != expectedToken || action == null) return false;
        action.run();
        return true;
    }
}
