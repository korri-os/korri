package com.limelight;

import org.junit.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class KorriUiStartGateTest {
    @Test
    public void localQueueTimeoutCancelsStartAndCannotExecuteLate() throws Exception {
        assertQueuedTimeoutDoesNotStart("local launcher start timed out");
    }

    @Test
    public void moonlightQueueTimeoutCancelsStartAndCannotExecuteLate() throws Exception {
        assertQueuedTimeoutDoesNotStart("Moonlight Activity start timed out");
    }

    @Test
    public void timeoutRaceWaitsForStartThatAlreadyCrossedTheGate() throws Exception {
        CountDownLatch startEntered = new CountDownLatch(1);
        CountDownLatch releaseStart = new CountDownLatch(1);
        ExecutorService callers = Executors.newSingleThreadExecutor();
        ExecutorService ui = Executors.newSingleThreadExecutor();
        KorriUiStartGate.Dispatcher queue = callback -> {
            ui.submit(callback);
            // dispatch must return only after Start has entered. The gate's
            // timeout then begins with STARTED established, not with a bet
            // that the test thread wins a 20 ms scheduling race.
            try {
                assertTrue(startEntered.await(5, TimeUnit.SECONDS));
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new AssertionError("UI start was interrupted");
            }
        };
        try {
            Future<?> result = callers.submit(() -> {
                KorriUiStartGate.run(queue, () -> {
                    startEntered.countDown();
                    releaseStart.await();
                }, 20, TimeUnit.MILLISECONDS, "timed out");
                return null;
            });

            assertTrue(startEntered.await(5, TimeUnit.SECONDS));
            try {
                result.get(100, TimeUnit.MILLISECONDS);
                fail("An executing start must remain pending until it completes");
            } catch (TimeoutException expected) {
                // The caller waits for the executing action past its timeout.
            }
            releaseStart.countDown();
            result.get(5, TimeUnit.SECONDS);
        } finally {
            releaseStart.countDown();
            ui.shutdownNow();
            callers.shutdownNow();
        }
    }

    @Test
    public void synchronousStartFailureIsReturnedToTheCaller() throws Exception {
        ManualUiQueue queue = new ManualUiQueue();
        IllegalStateException failure = new IllegalStateException("Android rejected start");
        ExecutorService callers = Executors.newSingleThreadExecutor();
        try {
            Future<?> result = callers.submit(() -> {
                KorriUiStartGate.run(queue, () -> {
                    throw failure;
                }, 1, TimeUnit.SECONDS, "timed out");
                return null;
            });

            queue.awaitQueued();
            queue.runQueued();
            try {
                result.get(1, TimeUnit.SECONDS);
                fail("start failure must reach caller");
            } catch (ExecutionException error) {
                assertSame(failure, error.getCause());
            }
        } finally {
            callers.shutdownNow();
        }
    }

    private static void assertQueuedTimeoutDoesNotStart(String message) throws Exception {
        ManualUiQueue queue = new ManualUiQueue();
        AtomicInteger starts = new AtomicInteger();
        ExecutorService callers = Executors.newSingleThreadExecutor();
        try {
            Future<?> result = callers.submit(() -> {
                KorriUiStartGate.run(
                        queue, starts::incrementAndGet, 20, TimeUnit.MILLISECONDS, message);
                return null;
            });

            queue.awaitQueued();
            try {
                result.get(1, TimeUnit.SECONDS);
                fail("queued start must time out");
            } catch (ExecutionException error) {
                assertEquals(message, error.getCause().getMessage());
            }
            queue.runQueued();
            assertEquals(0, starts.get());
        } finally {
            callers.shutdownNow();
        }
    }

    private static final class ManualUiQueue implements KorriUiStartGate.Dispatcher {
        private final CountDownLatch queued = new CountDownLatch(1);
        private Runnable callback;

        @Override
        public synchronized void dispatch(Runnable next) {
            callback = next;
            queued.countDown();
        }

        void awaitQueued() throws InterruptedException {
            assertTrue(queued.await(1, TimeUnit.SECONDS));
        }

        synchronized void runQueued() {
            callback.run();
        }
    }
}
