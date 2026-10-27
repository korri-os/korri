package com.limelight;

import com.limelight.nvstream.http.NvApp;

import org.junit.Test;

import java.util.ArrayDeque;
import java.util.Collections;
import java.util.Deque;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

public class KorriMoonlightDiscoveryTest {
    @Test
    public void emptyFirstPollPublishesCompletionAfterBackgroundRepair() throws Exception {
        NvApp refreshed = new NvApp("Korri Stream", "current", 7, false);
        AtomicReference<List<NvApp>> durable =
                new AtomicReference<>(Collections.emptyList());
        CountDownLatch releaseRefresh = new CountDownLatch(1);
        CountDownLatch completion = new CountDownLatch(1);
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                    hostUuid -> durable.get(),
                    (hostUuid, cached) -> cached.isEmpty(),
                    (hostUuid, guard) -> {
                        assertTrue(releaseRefresh.await(2, TimeUnit.SECONDS));
                        guard.commit(() -> {
                            durable.set(Collections.singletonList(refreshed));
                            return true;
                        });
                    },
                    hostUuid -> completion.countDown(),
                    executor,
                    System::currentTimeMillis);

            assertTrue(discovery.query("host-uuid").isEmpty());
            releaseRefresh.countDown();
            assertTrue(completion.await(2, TimeUnit.SECONDS));
            List<NvApp> visible = discovery.query("host-uuid");
            assertEquals(1, visible.size());
            assertSame(refreshed, visible.get(0));
            discovery.close();
        } finally {
            releaseRefresh.countDown();
            executor.shutdownNow();
        }
    }

    @Test
    public void healthyHostDoesNoNetworkWork() throws Exception {
        NvApp app = new NvApp("Korri Stream", "current", 7, false);
        AtomicInteger refreshes = new AtomicInteger();
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.singletonList(app),
                (hostUuid, cached) -> false,
                (hostUuid, guard) -> refreshes.incrementAndGet(),
                hostUuid -> {},
                Runnable::run,
                System::currentTimeMillis);

        assertEquals(1, discovery.query("healthy").size());
        assertEquals(0, refreshes.get());
        discovery.close();
    }

    @Test
    public void closeFencesWritesAndCompletion() throws Exception {
        CountDownLatch refreshEntered = new CountDownLatch(1);
        CountDownLatch releaseRefresh = new CountDownLatch(1);
        AtomicInteger writes = new AtomicInteger();
        AtomicInteger completions = new AtomicInteger();
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                    hostUuid -> Collections.emptyList(),
                    (hostUuid, cached) -> true,
                    (hostUuid, guard) -> {
                        refreshEntered.countDown();
                        releaseRefresh.await(2, TimeUnit.SECONDS);
                        guard.commit(() -> {
                            writes.incrementAndGet();
                            return true;
                        });
                    },
                    hostUuid -> completions.incrementAndGet(),
                    executor,
                    System::currentTimeMillis);
            discovery.query("host-uuid");
            assertTrue(refreshEntered.await(2, TimeUnit.SECONDS));
            discovery.close();
            releaseRefresh.countDown();
            executor.shutdown();
            assertTrue(executor.awaitTermination(2, TimeUnit.SECONDS));
            assertEquals(0, writes.get());
            assertEquals(0, completions.get());
        } finally {
            releaseRefresh.countDown();
            executor.shutdownNow();
        }
    }

    @Test
    public void closeWaitsForCommitLeaseThatAlreadyPassedPrecommit() throws Exception {
        CountDownLatch precommit = new CountDownLatch(1);
        CountDownLatch releaseMutation = new CountDownLatch(1);
        CountDownLatch closeReturned = new CountDownLatch(1);
        AtomicInteger writes = new AtomicInteger();
        ExecutorService executor = Executors.newSingleThreadExecutor();
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.emptyList(),
                (hostUuid, cached) -> true,
                (hostUuid, guard) -> guard.commit(() -> {
                    precommit.countDown();
                    assertTrue(releaseMutation.await(2, TimeUnit.SECONDS));
                    writes.incrementAndGet();
                    return true;
                }),
                hostUuid -> {},
                executor,
                System::currentTimeMillis);
        try {
            discovery.query("host-uuid");
            assertTrue(precommit.await(2, TimeUnit.SECONDS));
            Thread closer = new Thread(() -> {
                discovery.close();
                closeReturned.countDown();
            });
            closer.start();
            assertFalse(closeReturned.await(100, TimeUnit.MILLISECONDS));
            releaseMutation.countDown();
            assertTrue(closeReturned.await(2, TimeUnit.SECONDS));
            assertEquals(1, writes.get());
            closer.join(2000);
        } finally {
            releaseMutation.countDown();
            discovery.close();
            executor.shutdownNow();
        }
    }

    @Test
    public void transientFailureRetriesAndReloadsWithoutAnotherQuery() throws Exception {
        ManualRetryScheduler scheduler = new ManualRetryScheduler();
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.emptyList(),
                (hostUuid, cached) -> true,
                (hostUuid, guard) -> {
                    if (attempts.incrementAndGet() == 1) {
                        throw new IllegalStateException("offline");
                    }
                },
                hostUuid -> completions.incrementAndGet(),
                Runnable::run,
                scheduler,
                1,
                System::currentTimeMillis);

        discovery.query("transient");
        assertEquals(1, attempts.get());
        assertEquals(1, scheduler.pending());
        scheduler.runNext();
        assertEquals(2, attempts.get());
        assertEquals(1, completions.get());
        discovery.close();
    }

    @Test
    public void exhaustedRetryBurstAllowsLaterQueryAfterCooldown() throws Exception {
        ManualRetryScheduler scheduler = new ManualRetryScheduler();
        AtomicInteger attempts = new AtomicInteger();
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.emptyList(),
                (hostUuid, cached) -> true,
                (hostUuid, guard) -> {
                    if (attempts.incrementAndGet() <= 4) {
                        throw new IllegalStateException("offline");
                    }
                },
                hostUuid -> completions.incrementAndGet(),
                Runnable::run,
                scheduler,
                1,
                System::currentTimeMillis);

        discovery.query("offline");
        scheduler.runNext();
        scheduler.runNext();
        scheduler.runNext();
        assertEquals(4, attempts.get());

        discovery.query("offline");
        assertEquals(4, attempts.get());

        scheduler.runNext();
        assertEquals(1, completions.get());
        discovery.query("offline");
        assertEquals(5, attempts.get());
        assertEquals(2, completions.get());
        discovery.close();
    }

    @Test
    public void saturatedQueueRetriesValidHostAfterCapacityReturns() throws Exception {
        CountDownLatch blockedEntered = new CountDownLatch(1);
        CountDownLatch releaseBlocked = new CountDownLatch(1);
        CountDownLatch fillerCompleted = new CountDownLatch(1);
        CountDownLatch validCompleted = new CountDownLatch(1);
        ManualRetryScheduler scheduler = new ManualRetryScheduler();
        ThreadPoolExecutor executor = new ThreadPoolExecutor(
                1, 1, 30, TimeUnit.SECONDS, new ArrayBlockingQueue<>(1));
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.emptyList(),
                (hostUuid, cached) -> true,
                (hostUuid, guard) -> {
                    if ("blocked".equals(hostUuid)) {
                        blockedEntered.countDown();
                        releaseBlocked.await(2, TimeUnit.SECONDS);
                    } else if ("filler".equals(hostUuid)) {
                        fillerCompleted.countDown();
                    }
                },
                hostUuid -> {
                    if ("valid".equals(hostUuid)) validCompleted.countDown();
                },
                executor,
                scheduler,
                1,
                System::currentTimeMillis);
        try {
            discovery.query("blocked");
            assertTrue(blockedEntered.await(2, TimeUnit.SECONDS));
            discovery.query("filler");
            discovery.query("valid");
            assertEquals(1, scheduler.pending());

            releaseBlocked.countDown();
            assertTrue(fillerCompleted.await(2, TimeUnit.SECONDS));
            scheduler.runNext();
            assertTrue(validCompleted.await(2, TimeUnit.SECONDS));
        } finally {
            releaseBlocked.countDown();
            discovery.close();
            executor.shutdownNow();
        }
    }

    @Test
    public void closeCancelsPendingAutomaticRetry() throws Exception {
        ManualRetryScheduler scheduler = new ManualRetryScheduler();
        AtomicInteger attempts = new AtomicInteger();
        KorriMoonlightDiscovery discovery = new KorriMoonlightDiscovery(
                hostUuid -> Collections.emptyList(),
                (hostUuid, cached) -> true,
                (hostUuid, guard) -> {
                    attempts.incrementAndGet();
                    throw new IllegalStateException("offline");
                },
                hostUuid -> {},
                Runnable::run,
                scheduler,
                1,
                System::currentTimeMillis);
        discovery.query("offline");
        assertEquals(1, scheduler.pending());
        discovery.close();
        scheduler.runNext();
        assertEquals(1, attempts.get());
    }

    private static final class ManualRetryScheduler
            implements KorriMoonlightDiscovery.RetryScheduler {
        private final Deque<Scheduled> pending = new ArrayDeque<>();

        @Override
        public synchronized KorriMoonlightDiscovery.RetryHandle schedule(
                Runnable callback, long delayMs) {
            Scheduled scheduled = new Scheduled(callback);
            pending.addLast(scheduled);
            return () -> {
                synchronized (ManualRetryScheduler.this) {
                    scheduled.cancelled.set(true);
                }
            };
        }

        synchronized int pending() {
            int count = 0;
            for (Scheduled scheduled : pending) {
                if (!scheduled.cancelled.get()) count++;
            }
            return count;
        }

        void runNext() {
            Scheduled scheduled;
            synchronized (this) {
                scheduled = pending.removeFirst();
            }
            if (!scheduled.cancelled.get()) scheduled.callback.run();
        }
    }

    private static final class Scheduled {
        final Runnable callback;
        final AtomicBoolean cancelled = new AtomicBoolean();

        Scheduled(Runnable callback) {
            this.callback = callback;
        }
    }
}
