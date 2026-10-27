package com.limelight.korri.moonlight;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.SunshineRuntimeSettingsSnapshot;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.locks.ReentrantReadWriteLock;
import java.util.function.Supplier;

/** Exact-session owner for Korri's Sunshine live-settings controls. */
public final class KorriSunshineRuntimeSettings implements AutoCloseable {
    private static final long POLL_MS = 50;
    private static final long MUTATION_WAIT_MS = 3400;
    private static final int MAX_QUERY_ATTEMPTS = 3;
    private static final int MAX_BITRATE = 150000;
    private static final int MAX_FPS = 240;
    private static final int MAX_DIMENSION = 8192;

    public enum MutationResult { APPLIED, FAILED, STALE }
    public interface Authorization {
        boolean isCurrent();
        default <T> T commit(Supplier<T> action, T staleResult) {
            if (!isCurrent()) return staleResult;
            return action.get();
        }
    }
    public interface NativeApi {
        SunshineRuntimeSettingsSnapshot snapshot();
        int query(long expectedEpoch, int requestId);
        int setBitrate(long expectedEpoch, int requestId, int value);
        int setFps(long expectedEpoch, int requestId, int value);
        int setResolution(long expectedEpoch, int requestId, int width, int height);
    }
    public interface Scheduler { void postDelayed(Runnable action, long delayMs); void remove(Runnable action); }
    public interface Listener { void changed(KorriSunshineRuntimeSettings source); }
    public interface Clock { long nowMs(); }
    public interface Waiter { boolean pause(long delayMs, CountDownLatch mutationSignal) throws InterruptedException; }
    public interface EpochQueryBudget { boolean tryAcquire(long epoch, int limit); }

    /** Process-wide retry ownership. A new Java owner cannot reset a live native epoch's budget. */
    public static final class ProcessEpochQueryBudget implements EpochQueryBudget {
        private long newestEpoch;
        private int attempts;

        @Override
        public synchronized boolean tryAcquire(long requestedEpoch, int limit) {
            if (requestedEpoch <= 0 || limit <= 0) return false;
            if (newestEpoch == 0 || Long.compareUnsigned(requestedEpoch, newestEpoch) > 0) {
                newestEpoch = requestedEpoch;
                attempts = 0;
            } else if (Long.compareUnsigned(requestedEpoch, newestEpoch) < 0) {
                return false;
            }
            if (attempts >= limit) return false;
            attempts++;
            return true;
        }
    }

    private static final EpochQueryBudget PROCESS_QUERY_BUDGET = new ProcessEpochQueryBudget();

    public static NativeApi moonBridge() {
        return new NativeApi() {
            public SunshineRuntimeSettingsSnapshot snapshot() { return MoonBridge.getSunshineRuntimeSettingsSnapshot(); }
            public int query(long epoch, int id) { return MoonBridge.querySunshineRuntimeSettings(epoch, id); }
            public int setBitrate(long epoch, int id, int value) { return MoonBridge.setSunshineRuntimeBitrate(epoch, id, value); }
            public int setFps(long epoch, int id, int value) { return MoonBridge.setSunshineRuntimeFps(epoch, id, value); }
            public int setResolution(long epoch, int id, int width, int height) { return MoonBridge.setSunshineRuntimeResolution(epoch, id, width, height); }
        };
    }

    private final Object lock = new Object();
    private final ReentrantReadWriteLock dispatchGate = new ReentrantReadWriteLock(true);
    private final NativeApi nativeApi;
    private final Scheduler scheduler;
    private final Listener listener;
    private final Clock clock;
    private final Waiter waiter;
    private final EpochQueryBudget queryBudget;
    private final Runnable poll = this::poll;
    private final long sessionEpoch;
    private SunshineRuntimeSettingsSnapshot state;
    private int nextRequestId;
    private boolean scheduled;
    private boolean closed;
    private boolean terminal;
    private boolean mutationPending;
    private CountDownLatch activeMutationSignal;

    public KorriSunshineRuntimeSettings(NativeApi nativeApi, Scheduler scheduler, Listener listener) {
        this(nativeApi, scheduler, listener,
                android.os.SystemClock::elapsedRealtime,
                (delay, mutationSignal) -> mutationSignal.await(delay, TimeUnit.MILLISECONDS),
                PROCESS_QUERY_BUDGET);
    }

    KorriSunshineRuntimeSettings(NativeApi nativeApi, Scheduler scheduler, Listener listener,
                                 Clock clock, Waiter waiter) {
        this(nativeApi, scheduler, listener, clock, waiter, new ProcessEpochQueryBudget());
    }

    KorriSunshineRuntimeSettings(NativeApi nativeApi, Scheduler scheduler, Listener listener,
                                 Clock clock, Waiter waiter, EpochQueryBudget queryBudget) {
        this.nativeApi = nativeApi;
        this.scheduler = scheduler;
        this.listener = listener;
        this.clock = clock;
        this.waiter = waiter;
        this.queryBudget = queryBudget;
        SunshineRuntimeSettingsSnapshot initial = nativeApi.snapshot();
        state = initial;
        sessionEpoch = initial.sessionEpoch;
        long watermark = Math.max(initial.queryRequestId, initial.mutationRequestId);
        nextRequestId = watermark >= Integer.MAX_VALUE ? -1 : (int) watermark + 1;
    }

    public boolean start() {
        boolean attach;
        synchronized (lock) {
            if (closed) return false;
            if (!sameSessionLocked(state) || nextRequestId <= 0) {
                terminal = true;
                return false;
            }
            if (finalCapabilitySnapshotLocked(state)) return true;
            attach = state.queryOutcome == MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                    || state.mutationOutcome == MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                    || state.reconciliationRequired;
        }
        if (attach) {
            schedule(POLL_MS);
            return true;
        }
        return submitQueryOrRetry(0);
    }

    public boolean terminal() { synchronized (lock) { return terminal; } }

    public boolean available(int operation) {
        synchronized (lock) { return availableLocked(operation); }
    }

    public int bitrate() { synchronized (lock) { return checkedPositive(state.currentBitrateKbps, MAX_BITRATE); } }
    public int bitrateMin() { synchronized (lock) { return checkedPositive(state.minBitrateKbps, MAX_BITRATE); } }
    public int bitrateMax() { synchronized (lock) { return checkedPositive(state.maxBitrateKbps, MAX_BITRATE); } }
    public int fps() { synchronized (lock) { return checkedPositive(state.currentFps, fpsLimitLocked(state)); } }
    public int fpsMax() { synchronized (lock) { return fpsLimitLocked(state); } }
    public int width() { synchronized (lock) { return checkedPositive(state.currentWidth, MAX_DIMENSION); } }
    public int widthMax() { synchronized (lock) { return checkedPositive(state.launchWidth, MAX_DIMENSION); } }

    public MutationResult setBitrate(int value, Authorization authorization) {
        return mutate(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS, value, 0, authorization);
    }

    public MutationResult restoreBitrate(Authorization authorization) {
        int launch;
        synchronized (lock) { launch = checkedPositive(state.launchBitrateKbps, MAX_BITRATE); }
        return mutate(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS, launch, 0, authorization);
    }

    public MutationResult setFps(int value, Authorization authorization) {
        return mutate(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS, value, 0, authorization);
    }

    public MutationResult restoreFps(Authorization authorization) {
        int launch;
        synchronized (lock) { launch = checkedPositive(state.launchFps, MAX_FPS); }
        return mutate(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS, launch, 0, authorization);
    }

    public MutationResult setWidth(int width, Authorization authorization) {
        int height;
        synchronized (lock) {
            height = safeHeight(width,
                    checkedPositive(state.launchWidth, MAX_DIMENSION),
                    checkedPositive(state.launchHeight, MAX_DIMENSION));
        }
        return height <= 0 ? MutationResult.FAILED : mutate(
                MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION,
                width, height, authorization);
    }

    public MutationResult restoreResolution(Authorization authorization) {
        int width;
        int height;
        synchronized (lock) {
            width = checkedPositive(state.launchWidth, MAX_DIMENSION);
            height = checkedPositive(state.launchHeight, MAX_DIMENSION);
        }
        return mutate(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION,
                width, height, authorization);
    }

    private MutationResult mutate(
            int operation, int value, int secondary, Authorization authorization) {
        if (authorization == null || !authorization.isCurrent()) return MutationResult.STALE;

        final int requestId;
        final CountDownLatch mutationSignal = new CountDownLatch(1);
        synchronized (lock) {
            if (!availableLocked(operation)) return MutationResult.STALE;
            if (!valueWithinBoundsLocked(operation, value, secondary)) return MutationResult.FAILED;
            requestId = takeRequestIdLocked();
            mutationPending = true;
            activeMutationSignal = mutationSignal;
        }

        Integer committedSendResult = authorization.commit(() -> {
            dispatchGate.readLock().lock();
            try {
                synchronized (lock) {
                    if (closed || terminal || !sameSessionLocked(state)
                            || activeMutationSignal != mutationSignal) {
                        return null;
                    }
                }
                if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS) {
                    return nativeApi.setBitrate(sessionEpoch, requestId, value);
                } else if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS) {
                    return nativeApi.setFps(sessionEpoch, requestId, value);
                } else {
                    return nativeApi.setResolution(
                            sessionEpoch, requestId, value, secondary);
                }
            } finally {
                dispatchGate.readLock().unlock();
            }
        }, null);
        if (committedSendResult == null) {
            finishLocalMutation(mutationSignal);
            return MutationResult.STALE;
        }
        int sendResult = committedSendResult;

        SunshineRuntimeSettingsSnapshot afterSend = nativeApi.snapshot();
        boolean changed;
        MutationResult immediate;
        synchronized (lock) {
            changed = updateStateLocked(afterSend);
            immediate = mutationResultLocked(requestId, sendResult, mutationSignal);
        }
        notifyChanged(changed);
        if (immediate != null) return immediate;
        schedule(POLL_MS);

        // Native transport acceptance is the commit point. Publication revocation and
        // Java-owner close can suppress UI delivery, but cannot rewrite host truth.
        long started = clock.nowMs();
        while (true) {
            SunshineRuntimeSettingsSnapshot next = nativeApi.snapshot();
            MutationResult result;
            synchronized (lock) {
                changed = updateStateLocked(next);
                result = mutationResultLocked(requestId, 0, mutationSignal);
            }
            notifyChanged(changed);
            if (result != null) return result;

            long now = clock.nowMs();
            if (now - started >= MUTATION_WAIT_MS) {
                finishLocalMutation(mutationSignal);
                return MutationResult.FAILED;
            }
            try {
                waiter.pause(Math.min(25, MUTATION_WAIT_MS - (now - started)),
                        mutationSignal);
            } catch (InterruptedException error) {
                // Teardown may interrupt its executor. The accepted host command still
                // owns a bounded exact-epoch reconciliation obligation.
                Thread.interrupted();
            }
        }
    }

    private MutationResult mutationResultLocked(
            int requestId, int sendResult, CountDownLatch mutationSignal) {
        if (sendResult == MoonBridge.LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION) {
            finishLocalMutationLocked(mutationSignal);
            return MutationResult.STALE;
        }
        if (sendResult != 0) {
            finishLocalMutationLocked(mutationSignal);
            return MutationResult.FAILED;
        }
        if (state.sessionEpoch != sessionEpoch || state.mutationRequestId != requestId) {
            finishLocalMutationLocked(mutationSignal);
            return MutationResult.STALE;
        }
        switch (state.mutationOutcome) {
            case MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED:
                finishLocalMutationLocked(mutationSignal);
                return state.mutationStatus == MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED
                        && state.mutationReason == MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE
                        ? MutationResult.APPLIED : MutationResult.FAILED;
            case MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_REJECTED:
            case MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT:
            case MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_SEND_FAILED:
            case MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED:
                finishLocalMutationLocked(mutationSignal);
                return MutationResult.FAILED;
            default:
                if (!state.sessionActive) {
                    finishLocalMutationLocked(mutationSignal);
                    return MutationResult.STALE;
                }
                return null;
        }
    }

    private void finishLocalMutation(CountDownLatch mutationSignal) {
        synchronized (lock) { finishLocalMutationLocked(mutationSignal); }
    }

    private void finishLocalMutationLocked(CountDownLatch mutationSignal) {
        if (activeMutationSignal != mutationSignal) return;
        mutationPending = false;
        activeMutationSignal = null;
    }

    private boolean submitQueryOrRetry(long delay) {
        if (delay > 0) {
            schedule(delay);
            return true;
        }

        long epoch;
        synchronized (lock) {
            if (closed || terminal || !sameSessionLocked(state) || nextRequestId <= 0) {
                terminal = true;
                return false;
            }
            epoch = sessionEpoch;
        }
        if (!queryBudget.tryAcquire(epoch, MAX_QUERY_ATTEMPTS)) {
            synchronized (lock) { terminal = true; }
            return false;
        }

        final int requestId;
        final int result;
        dispatchGate.readLock().lock();
        try {
            synchronized (lock) {
                if (closed || terminal || !sameSessionLocked(state) || nextRequestId <= 0) {
                    terminal = true;
                    return false;
                }
                requestId = takeRequestIdLocked();
            }
            result = nativeApi.query(epoch, requestId);
        } finally {
            dispatchGate.readLock().unlock();
        }

        SunshineRuntimeSettingsSnapshot next = nativeApi.snapshot();
        boolean changed;
        boolean retry = false;
        boolean accepted = result == 0;
        synchronized (lock) {
            changed = updateStateLocked(next);
            if (closed || !sameSessionLocked(state)
                    || result == MoonBridge.LI_RUNTIME_SETTINGS_ERROR_NOT_SUNSHINE
                    || result == MoonBridge.LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION) {
                terminal = true;
            } else if (!accepted) {
                retry = true;
            }
        }
        notifyChanged(changed);
        if (accepted) {
            schedule(POLL_MS);
            return true;
        }
        if (retry) {
            schedule(50);
            return true;
        }
        return false;
    }

    private boolean finalCapabilitySnapshotLocked(SunshineRuntimeSettingsSnapshot value) {
        return sameSessionLocked(value)
                && value.capabilityReceived
                && value.queryRequestId > 0
                && value.queryOutcome == MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED
                && value.mutationOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                && !value.reconciliationRequired
                && factsRepresentable(value);
    }

    private boolean availableLocked(int operation) {
        SunshineRuntimeSettingsSnapshot value = state;
        return !closed && !terminal && !mutationPending && nextRequestId > 0
                && sameSessionLocked(value) && factsRepresentable(value)
                && value.capabilityReceived
                && value.capabilityStatus == MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED
                && value.capabilityReason == MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE
                && value.queryOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                && value.mutationOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                && !value.reconciliationRequired
                && (value.supportedOperations & (1L << operation)) != 0
                && liveFactsValidLocked(operation, value);
    }

    private boolean valueWithinBoundsLocked(int operation, int value, int secondary) {
        if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS) {
            return value >= checkedPositive(state.minBitrateKbps, MAX_BITRATE)
                    && value <= checkedPositive(state.maxBitrateKbps, MAX_BITRATE);
        }
        if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS) {
            return value >= 1 && value <= fpsLimitLocked(state);
        }
        return value >= 2 && value <= checkedPositive(state.launchWidth, MAX_DIMENSION)
                && (value & 1) == 0 && secondary > 0;
    }

    private boolean liveFactsValidLocked(int operation, SunshineRuntimeSettingsSnapshot value) {
        if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS) {
            int minimum = checkedPositive(value.minBitrateKbps, MAX_BITRATE);
            int maximum = checkedPositive(value.maxBitrateKbps, MAX_BITRATE);
            int current = checkedPositive(value.currentBitrateKbps, MAX_BITRATE);
            return minimum <= maximum && current >= minimum && current <= maximum;
        }
        if (operation == MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS) {
            int maximum = fpsLimitLocked(value);
            int current = checkedPositive(value.currentFps, MAX_FPS);
            return current >= 1 && current <= maximum;
        }
        int maximum = checkedPositive(value.launchWidth, MAX_DIMENSION);
        int current = checkedPositive(value.currentWidth, MAX_DIMENSION);
        return maximum >= 2 && current >= 2 && current <= maximum
                && (current & 1) == 0 && value.launchHeight > 0;
    }

    private static boolean factsRepresentable(SunshineRuntimeSettingsSnapshot value) {
        return positive(value.launchBitrateKbps, MAX_BITRATE)
                && positive(value.currentBitrateKbps, MAX_BITRATE)
                && positive(value.minBitrateKbps, MAX_BITRATE)
                && positive(value.maxBitrateKbps, MAX_BITRATE)
                && positive(value.launchFps, MAX_FPS)
                && positive(value.currentFps, MAX_FPS)
                && positive(value.maxFps, MAX_FPS)
                && positive(value.launchWidth, MAX_DIMENSION)
                && positive(value.launchHeight, MAX_DIMENSION)
                && positive(value.currentWidth, MAX_DIMENSION)
                && positive(value.currentHeight, MAX_DIMENSION);
    }

    private static boolean positive(long value, int max) {
        return value > 0 && value <= max && value <= Integer.MAX_VALUE;
    }

    private static int checkedPositive(long value, int max) {
        if (!positive(value, max)) throw new IllegalStateException("Invalid runtime-settings fact");
        return (int) value;
    }

    private static int fpsLimitLocked(SunshineRuntimeSettingsSnapshot value) {
        return Math.min(
                checkedPositive(value.maxFps, MAX_FPS),
                checkedPositive(value.launchFps, MAX_FPS));
    }

    static int safeHeight(int width, int launchWidth, int launchHeight) {
        if (width <= 0 || launchWidth <= 0 || launchHeight <= 0
                || width > MAX_DIMENSION || launchWidth > MAX_DIMENSION
                || launchHeight > MAX_DIMENSION || (width & 1) != 0
                || width > launchWidth) return -1;
        long numerator = (long) width * (long) launchHeight;
        int rounded = (int) ((numerator + (long) launchWidth / 2L) / (long) launchWidth);
        int low = rounded & ~1;
        int high = low + 2;
        int best = -1;
        long bestDelta = Long.MAX_VALUE;
        for (int height : new int[] {low, high}) {
            if (height <= 0 || height > launchHeight) continue;
            long delta = Math.abs((long) width * (long) launchHeight
                    - (long) height * (long) launchWidth);
            long tolerance = 2L * (long) launchWidth + 2L * (long) launchHeight;
            if (delta <= tolerance && delta < bestDelta) {
                best = height;
                bestDelta = delta;
            }
        }
        return best;
    }

    private int takeRequestIdLocked() {
        if (nextRequestId <= 0) {
            throw new IllegalStateException("Runtime-settings request IDs exhausted");
        }
        int value = nextRequestId;
        nextRequestId = value == Integer.MAX_VALUE ? -1 : value + 1;
        return value;
    }

    private boolean sameSessionLocked(SunshineRuntimeSettingsSnapshot value) {
        return value.sessionActive && value.sessionEpoch == sessionEpoch && sessionEpoch > 0;
    }

    private boolean updateStateLocked(SunshineRuntimeSettingsSnapshot next) {
        // Once native truth leaves this owner's exact epoch, a delayed snapshot from
        // the retired epoch cannot resurrect it.
        if (state.sessionEpoch != sessionEpoch && next.sessionEpoch == sessionEpoch) {
            return false;
        }
        boolean sameSession = next.sessionEpoch == state.sessionEpoch;
        if (sameSession && Long.compareUnsigned(next.generation, state.generation) < 0) {
            return false;
        }
        boolean changed = next.generation != state.generation
                || next.sessionEpoch != state.sessionEpoch
                || next.sessionActive != state.sessionActive;
        state = next;
        return changed;
    }

    boolean acceptSnapshot(SunshineRuntimeSettingsSnapshot next) {
        boolean changed;
        synchronized (lock) { changed = updateStateLocked(next); }
        notifyChanged(changed);
        return changed;
    }

    private void notifyChanged(boolean changed) {
        boolean notify;
        synchronized (lock) { notify = changed && !closed; }
        if (notify) listener.changed(this);
    }

    private void poll() {
        synchronized (lock) {
            scheduled = false;
            if (closed || terminal) return;
        }

        SunshineRuntimeSettingsSnapshot next = nativeApi.snapshot();
        boolean changed;
        boolean query = false;
        boolean keepPolling = false;
        synchronized (lock) {
            changed = updateStateLocked(next);
            if (!sameSessionLocked(state)) {
                terminal = true;
            } else if ((!state.capabilityReceived
                    && state.queryOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT)
                    || (state.reconciliationRequired
                    && state.queryOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                    && state.mutationOutcome != MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT)) {
                query = true;
            } else if (state.capabilityReceived
                    && (state.capabilityStatus == MoonBridge.SS_RUNTIME_SETTINGS_STATUS_DISABLED
                    || state.capabilityReason == MoonBridge.SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER)) {
                terminal = true;
            } else {
                keepPolling = state.queryOutcome == MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                        || state.mutationOutcome == MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT
                        || state.reconciliationRequired;
            }
        }
        notifyChanged(changed);
        if (query) submitQueryOrRetry(0);
        else if (keepPolling) schedule(POLL_MS);
    }

    private void schedule(long delay) {
        synchronized (lock) {
            if (closed || terminal || scheduled) return;
            scheduled = true;
        }
        scheduler.postDelayed(poll, delay);
        boolean cancel;
        synchronized (lock) { cancel = closed || terminal || !scheduled; }
        if (cancel) scheduler.remove(poll);
    }

    @Override
    public void close() {
        boolean cancel;
        dispatchGate.writeLock().lock();
        try {
            synchronized (lock) {
                if (closed) return;
                closed = true;
                cancel = scheduled;
                scheduled = false;
            }
        } finally {
            dispatchGate.writeLock().unlock();
        }
        if (cancel) scheduler.remove(poll);
    }
}
