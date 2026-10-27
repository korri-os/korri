package com.limelight;

import com.limelight.nvstream.http.NvApp;

import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executor;
import java.util.concurrent.ScheduledThreadPoolExecutor;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

/** Returns current app state and repairs Moonlight trust away from the WebView thread. */
final class KorriMoonlightDiscovery implements AutoCloseable {
    private static final int MAX_PENDING_HOSTS = 16;
    private static final int WORKER_COUNT = 4;
    private static final int MAX_RETRY_ATTEMPTS = 4;
    private static final long RETRY_BACKOFF_MS = 5_000;
    private static final long MAX_RETRY_BACKOFF_MS = 20_000;
    private static final Executor ANDROID_REFRESH_EXECUTOR = createAndroidExecutor();
    private static final RetryScheduler ANDROID_RETRY_SCHEDULER = createAndroidRetryScheduler();

    interface CachedApps {
        List<NvApp> read(String hostUuid) throws Exception;
    }

    interface NeedsRepair {
        boolean test(String hostUuid, List<NvApp> cached) throws Exception;
    }

    interface Commit<T> {
        T run() throws Exception;
    }

    /** A lifecycle guard whose commit lease is mutually exclusive with close(). */
    interface Guard {
        boolean current();

        default <T> T commit(Commit<T> action) throws Exception {
            if (!current()) return null;
            return action.run();
        }
    }

    interface Refresh {
        void run(String hostUuid, Guard guard) throws Exception;
    }

    interface Completion {
        void appsChanged(String hostUuid);
    }

    interface Clock {
        long nowMs();
    }

    interface RetryHandle {
        void cancel();
    }

    interface RetryScheduler {
        RetryHandle schedule(Runnable callback, long delayMs);
    }

    private final CachedApps cachedApps;
    private final NeedsRepair needsRepair;
    private final Refresh refresh;
    private final Completion completion;
    private final Executor executor;
    private final RetryScheduler retryScheduler;
    private final long retryBackoffMs;
    private final Clock clock;
    private final ConcurrentHashMap<String, Boolean> inFlight = new ConcurrentHashMap<>();
    private final ConcurrentHashMap<String, Integer> retryAttempts = new ConcurrentHashMap<>();
    private final ConcurrentHashMap<String, RetryHandle> scheduledRetries =
            new ConcurrentHashMap<>();
    private final AtomicLong generation = new AtomicLong(1);
    private final Object lifecycleMonitor = new Object();
    private boolean closed;

    KorriMoonlightDiscovery(
            CachedApps cachedApps,
            NeedsRepair needsRepair,
            Refresh refresh,
            Completion completion) {
        this(cachedApps, needsRepair, refresh, completion,
                ANDROID_REFRESH_EXECUTOR, ANDROID_RETRY_SCHEDULER,
                RETRY_BACKOFF_MS, android.os.SystemClock::elapsedRealtime);
    }

    KorriMoonlightDiscovery(
            CachedApps cachedApps,
            NeedsRepair needsRepair,
            Refresh refresh,
            Completion completion,
            Executor executor,
            Clock clock) {
        this(cachedApps, needsRepair, refresh, completion,
                executor, ANDROID_RETRY_SCHEDULER, RETRY_BACKOFF_MS, clock);
    }

    KorriMoonlightDiscovery(
            CachedApps cachedApps,
            NeedsRepair needsRepair,
            Refresh refresh,
            Completion completion,
            Executor executor,
            RetryScheduler retryScheduler,
            long retryBackoffMs,
            Clock clock) {
        this.cachedApps = cachedApps;
        this.needsRepair = needsRepair;
        this.refresh = refresh;
        this.completion = completion;
        this.executor = executor;
        this.retryScheduler = retryScheduler;
        this.retryBackoffMs = retryBackoffMs;
        this.clock = clock;
    }

    List<NvApp> query(String hostUuid) throws Exception {
        List<NvApp> current = cachedApps.read(hostUuid);
        if (needsRepair.test(hostUuid, current)) {
            schedule(hostUuid, generation.get());
        }
        return current;
    }

    private void schedule(String hostUuid, long ticket) {
        if (hostUuid == null || hostUuid.trim().isEmpty() || !isCurrent(ticket)) {
            return;
        }
        if (retryAttempts.getOrDefault(hostUuid, 0) >= MAX_RETRY_ATTEMPTS) {
            return;
        }
        if (inFlight.putIfAbsent(hostUuid, Boolean.TRUE) != null) {
            return;
        }
        try {
            executor.execute(() -> runAttempt(hostUuid, ticket));
        } catch (RuntimeException rejected) {
            inFlight.remove(hostUuid);
            requestRetry(hostUuid, ticket);
        }
    }

    private void runAttempt(String hostUuid, long ticket) {
        Guard guard = guard(ticket);
        boolean succeeded = false;
        try {
            if (!guard.current()) return;
            refresh.run(hostUuid, guard);
            if (!guard.current()) return;
            retryAttempts.remove(hostUuid);
            cancelRetry(hostUuid);
            completion.appsChanged(hostUuid);
            succeeded = true;
        } catch (Exception ignored) {
            // The bounded retry below is the only recovery signal. Certificate
            // material and host errors must not enter logs from this layer.
        } finally {
            inFlight.remove(hostUuid);
            if (!succeeded && guard.current()) {
                requestRetry(hostUuid, ticket);
            }
        }
    }

    private void requestRetry(String hostUuid, long ticket) {
        if (!isCurrent(ticket)) return;
        int attempt = retryAttempts.merge(hostUuid, 1, Integer::sum);
        boolean burstExhausted = attempt >= MAX_RETRY_ATTEMPTS;
        long multiplier = 1L << Math.min(attempt - 1, 2);
        long delay = burstExhausted
                ? MAX_RETRY_BACKOFF_MS
                : Math.min(MAX_RETRY_BACKOFF_MS, retryBackoffMs * multiplier);
        try {
            RetryHandle handle = retryScheduler.schedule(() -> {
                scheduledRetries.remove(hostUuid);
                if (!isCurrent(ticket)) return;
                if (burstExhausted) {
                    // One burst is bounded, but a host that comes online later
                    // must not stay disabled for this Activity lifetime. Wake
                    // the portal after the cooldown so its next query starts a
                    // fresh bounded burst.
                    retryAttempts.remove(hostUuid);
                    completion.appsChanged(hostUuid);
                } else {
                    schedule(hostUuid, ticket);
                }
            }, delay);
            RetryHandle previous = scheduledRetries.put(hostUuid, handle);
            if (previous != null) previous.cancel();
        } catch (RuntimeException rejected) {
            // Wake the portal once so its normal discovery call can retry. The
            // retry-attempt cap still prevents an unbounded immediate loop.
            if (burstExhausted) retryAttempts.remove(hostUuid);
            if (isCurrent(ticket)) completion.appsChanged(hostUuid);
        }
    }

    private Guard guard(long ticket) {
        return new Guard() {
            @Override
            public boolean current() {
                return isCurrent(ticket);
            }

            @Override
            public <T> T commit(Commit<T> action) throws Exception {
                synchronized (lifecycleMonitor) {
                    if (closed || generation.get() != ticket) return null;
                    return action.run();
                }
            }
        };
    }

    private boolean isCurrent(long ticket) {
        synchronized (lifecycleMonitor) {
            return !closed && generation.get() == ticket;
        }
    }

    private void cancelRetry(String hostUuid) {
        RetryHandle handle = scheduledRetries.remove(hostUuid);
        if (handle != null) handle.cancel();
    }

    @Override
    public void close() {
        synchronized (lifecycleMonitor) {
            if (closed) return;
            closed = true;
            generation.incrementAndGet();
        }
        for (RetryHandle handle : scheduledRetries.values()) handle.cancel();
        scheduledRetries.clear();
        inFlight.clear();
        retryAttempts.clear();
    }

    private static Executor createAndroidExecutor() {
        ThreadFactory factory = runnable -> {
            Thread thread = new Thread(runnable, "korri-moonlight-discovery");
            thread.setDaemon(true);
            return thread;
        };
        ThreadPoolExecutor executor = new ThreadPoolExecutor(
                WORKER_COUNT,
                WORKER_COUNT,
                30,
                TimeUnit.SECONDS,
                new ArrayBlockingQueue<>(MAX_PENDING_HOSTS),
                factory,
                new ThreadPoolExecutor.AbortPolicy());
        executor.allowCoreThreadTimeOut(true);
        return executor;
    }

    private static RetryScheduler createAndroidRetryScheduler() {
        ThreadFactory factory = runnable -> {
            Thread thread = new Thread(runnable, "korri-moonlight-discovery-retry");
            thread.setDaemon(true);
            return thread;
        };
        ScheduledThreadPoolExecutor scheduler = new ScheduledThreadPoolExecutor(1, factory);
        scheduler.setRemoveOnCancelPolicy(true);
        scheduler.setExecuteExistingDelayedTasksAfterShutdownPolicy(false);
        return (callback, delayMs) -> {
            java.util.concurrent.ScheduledFuture<?> future =
                    scheduler.schedule(callback, delayMs, TimeUnit.MILLISECONDS);
            return () -> future.cancel(false);
        };
    }
}
