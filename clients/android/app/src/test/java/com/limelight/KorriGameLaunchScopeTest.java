package com.limelight;

import org.junit.Test;

import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;

public class KorriGameLaunchScopeTest {
    @Test
    public void retryFailurePreservesLaunchThroughConnectedUntilTerminalEnd() {
        AtomicInteger endings = new AtomicInteger();
        KorriGameLaunchScope scope = new KorriGameLaunchScope(endings::incrementAndGet);

        scope.stageFailed(true);
        scope.connectionStarted();
        assertEquals(0, endings.get());

        scope.connectionTerminated();
        assertEquals(1, endings.get());
    }

    @Test
    public void terminalStageFailureEndsLaunchAfterRetryDecision() {
        AtomicInteger endings = new AtomicInteger();
        KorriGameLaunchScope scope = new KorriGameLaunchScope(endings::incrementAndGet);

        scope.stageFailed(false);

        assertEquals(1, endings.get());
    }
}
