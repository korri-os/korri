package com.limelight;

/** Orders Moonlight callbacks so retryable setup failures do not end Korri's launch. */
final class KorriGameLaunchScope {
    private final Runnable endLaunch;

    KorriGameLaunchScope(Runnable endLaunch) {
        this.endLaunch = endLaunch;
    }

    void stageFailed(boolean retryable) {
        if (!retryable) {
            endLaunch.run();
        }
    }

    void connectionStarted() {
        // A successful retry continues the existing launch identity.
    }

    void connectionTerminated() {
        endLaunch.run();
    }
}
