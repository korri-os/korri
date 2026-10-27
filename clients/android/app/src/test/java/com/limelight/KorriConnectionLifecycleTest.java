package com.limelight;

import org.junit.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class KorriConnectionLifecycleTest {
    @Test
    public void terminalEndRejectsAlreadyPostedStartWork() {
        KorriConnectionLifecycle lifecycle = new KorriConnectionLifecycle();
        long postedStart = lifecycle.started();
        lifecycle.retire();
        assertFalse(lifecycle.isCurrent(postedStart));
    }

    @Test
    public void newerStartRejectsOlderStartWork() {
        KorriConnectionLifecycle lifecycle = new KorriConnectionLifecycle();
        long oldStart = lifecycle.started();
        long currentStart = lifecycle.started();
        assertFalse(lifecycle.isCurrent(oldStart));
        assertTrue(lifecycle.isCurrent(currentStart));
    }
    @Test
    public void retireWaitsForExactStartLeaseAndStaleWorkCannotRegister() throws Exception {
        KorriConnectionLifecycle lifecycle = new KorriConnectionLifecycle();
        long start = lifecycle.started();
        CountDownLatch entered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        CountDownLatch retired = new CountDownLatch(1);
        AtomicBoolean registered = new AtomicBoolean();

        Thread startWork = new Thread(() -> lifecycle.runIfCurrent(start, () -> {
            entered.countDown();
            try {
                release.await();
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                return;
            }
            registered.set(true);
        }));
        startWork.start();
        assertTrue(entered.await(1, TimeUnit.SECONDS));

        Thread terminal = new Thread(() -> {
            lifecycle.retire();
            retired.countDown();
        });
        terminal.start();
        assertFalse("retire must wait for the active start lease",
                retired.await(50, TimeUnit.MILLISECONDS));

        release.countDown();
        startWork.join(1000);
        assertTrue(retired.await(1, TimeUnit.SECONDS));
        assertTrue(registered.get());

        AtomicBoolean staleRegistration = new AtomicBoolean();
        assertFalse(lifecycle.runIfCurrent(start, () -> staleRegistration.set(true)));
        assertFalse(staleRegistration.get());
    }

}
