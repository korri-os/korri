package com.limelight.nvstream;

import org.junit.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.*;

public class NvConnectionLifecycleTest {
    private static final class NativeEffects implements NvConnectionLifecycle.Bridge {
        final AtomicInteger starts = new AtomicInteger();
        final AtomicInteger interrupts = new AtomicInteger();
        final AtomicInteger stops = new AtomicInteger();
        final AtomicInteger cleanups = new AtomicInteger();
        final CountDownLatch interrupted = new CountDownLatch(1);
        public void interrupt() { interrupts.incrementAndGet(); interrupted.countDown(); }
        public void stop() { stops.incrementAndGet(); }
        public void cleanup() { cleanups.incrementAndGet(); }
        int start() { starts.incrementAndGet(); return 0; }
    }

    @Test
    public void delayedBootstrapCannotStartNativeAfterConfirmedStop() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle life = new NvConnectionLifecycle(new NvConnectionLifecycle.Slot(new Object()), effects);
        CountDownLatch bootstrap = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        CountDownLatch stopped = new CountDownLatch(1);
        life.start(() -> {
            bootstrap.countDown();
            awaitIgnoringInterrupt(release);
            try { life.connect(effects::start); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        });
        assertTrue(bootstrap.await(1, TimeUnit.SECONDS));
        Thread stop = new Thread(() -> { life.stop(); stopped.countDown(); });
        stop.start();
        try {
            assertFalse("stop must drain the startup work", stopped.await(20, TimeUnit.MILLISECONDS));
        } finally {
            release.countDown();
        }
        assertTrue(stopped.await(1, TimeUnit.SECONDS));
        stop.join(1000);
        assertEquals(0, effects.starts.get());
        assertEquals(0, effects.interrupts.get());
        assertEquals(0, effects.stops.get());
    }

    @Test
    public void stoppingAQueuedConnectionNeverInterruptsTheActiveDecoder() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle.Slot slot = new NvConnectionLifecycle.Slot(new Object());
        NvConnectionLifecycle active = new NvConnectionLifecycle(slot, effects);
        CountDownLatch connected = start(active, effects::start);
        assertTrue(connected.await(1, TimeUnit.SECONDS));
        NvConnectionLifecycle queued = new NvConnectionLifecycle(slot, effects);
        CountDownLatch queuedStarted = new CountDownLatch(1);
        queued.start(() -> {
            queuedStarted.countDown();
            try { queued.connect(effects::start); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        });
        assertTrue(queuedStarted.await(1, TimeUnit.SECONDS));
        queued.stop();
        assertEquals(1, effects.starts.get());
        assertEquals(0, effects.interrupts.get());
        assertEquals(0, effects.stops.get());
        active.stop();
        assertEquals(1, effects.stops.get());
    }

    @Test
    public void lateAndRepeatedStopsCannotTouchTheReplacementOrInflateItsSlot() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle.Slot slot = new NvConnectionLifecycle.Slot(new Object());
        NvConnectionLifecycle first = new NvConnectionLifecycle(slot, effects);
        assertTrue(start(first, effects::start).await(1, TimeUnit.SECONDS));
        first.stop();
        first.stop();
        NvConnectionLifecycle second = new NvConnectionLifecycle(slot, effects);
        assertTrue(start(second, effects::start).await(1, TimeUnit.SECONDS));
        first.stop();
        assertEquals(1, effects.interrupts.get());
        assertEquals(1, effects.stops.get());
        assertEquals(2, effects.starts.get());
        second.stop();
        assertEquals(2, effects.stops.get());
    }

    @Test
    public void failedNativeStartCleansUpOnceWithoutStoppingAnotherOwner() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle.Slot slot = new NvConnectionLifecycle.Slot(new Object());
        NvConnectionLifecycle failed = new NvConnectionLifecycle(slot, effects);
        assertTrue(start(failed, () -> -1).await(1, TimeUnit.SECONDS));
        failed.stop();
        assertEquals(1, effects.cleanups.get());
        assertEquals(0, effects.stops.get());
        NvConnectionLifecycle next = new NvConnectionLifecycle(slot, effects);
        assertTrue(start(next, effects::start).await(1, TimeUnit.SECONDS));
        failed.stop();
        assertEquals(0, effects.interrupts.get());
        next.stop();
    }

    @Test
    public void stopInterruptsItsOwnInFlightNativeStartAndWaitsForCleanup() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle life = new NvConnectionLifecycle(new NvConnectionLifecycle.Slot(new Object()), effects);
        CountDownLatch entered = new CountDownLatch(1);
        life.start(() -> {
            try {
                life.connect(() -> {
                    entered.countDown();
                    awaitIgnoringInterrupt(effects.interrupted);
                    return 0;
                });
            } catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        });
        assertTrue(entered.await(1, TimeUnit.SECONDS));
        life.stop();
        assertEquals(1, effects.interrupts.get());
        assertEquals(1, effects.stops.get());
        assertEquals(1, effects.cleanups.get());
    }

    @Test
    public void cancellingAQueuedConnectionDoesNotWaitForAnotherNativeStartup() throws Exception {
        NativeEffects effects = new NativeEffects();
        NvConnectionLifecycle.Slot slot = new NvConnectionLifecycle.Slot(new Object());
        NvConnectionLifecycle active = new NvConnectionLifecycle(slot, effects);
        CountDownLatch entered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        active.start(() -> {
            try {
                active.connect(() -> { entered.countDown(); awaitIgnoringInterrupt(release); return 0; });
            } catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        });
        assertTrue(entered.await(1, TimeUnit.SECONDS));
        NvConnectionLifecycle queued = new NvConnectionLifecycle(slot, effects);
        CountDownLatch queuedStarted = new CountDownLatch(1);
        queued.start(() -> {
            queuedStarted.countDown();
            try { queued.connect(effects::start); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        });
        assertTrue(queuedStarted.await(1, TimeUnit.SECONDS));
        CountDownLatch stopped = new CountDownLatch(1);
        Thread stop = new Thread(() -> { queued.stop(); stopped.countDown(); });
        stop.start();
        try {
            assertTrue("queued stop cannot wait on somebody else's native monitor", stopped.await(1, TimeUnit.SECONDS));
            assertEquals(0, effects.interrupts.get());
        } finally {
            release.countDown();
            stop.join(2000);
            active.stop();
        }
    }

    private static CountDownLatch start(NvConnectionLifecycle life, NvConnectionLifecycle.NativeStart operation) {
        CountDownLatch done = new CountDownLatch(1);
        life.start(() -> {
            try { life.connect(operation); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); }
            finally { done.countDown(); }
        });
        return done;
    }

    private static void awaitIgnoringInterrupt(CountDownLatch latch) {
        for (;;) {
            try { latch.await(); return; }
            catch (InterruptedException ignored) { /* Models a non-cancellable HTTP bootstrap. */ }
        }
    }
}
