package com.limelight.korri.moonlight;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.SunshineRuntimeSettingsSnapshot;

import org.junit.Test;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.*;

public class KorriSunshineRuntimeSettingsTest {
    private static final KorriSunshineRuntimeSettings.Authorization CURRENT = () -> true;

    @Test
    public void appliedWaitsForDelayedExactHostAckAndNotifiesListener() throws Exception {
        FakeNative nativeApi = FakeNative.ready(1, 12345, 60, 60);
        FakeScheduler scheduler = new FakeScheduler();
        ControlledWaiter waiter = new ControlledWaiter();
        CountDownLatch changed = new CountDownLatch(1);
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, scheduler, ignored -> changed.countDown(), waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());

        Thread mutation = new Thread(() -> assertEquals(
                KorriSunshineRuntimeSettings.MutationResult.APPLIED,
                owner.setBitrate(12001, CURRENT)));
        mutation.start();
        assertTrue(nativeApi.mutationSent.await(1, TimeUnit.SECONDS));
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));
        assertTrue("transport acceptance is not execution", mutation.isAlive());

        nativeApi.completeMutation(
                MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE);
        waiter.advance();
        mutation.join(1000);
        assertFalse(mutation.isAlive());
        assertTrue(changed.await(1, TimeUnit.SECONDS));
    }

    @Test
    public void closeAfterAcceptedSendStillReportsAppliedHostTruth() throws Exception {
        FakeNative nativeApi = FakeNative.ready(2, 12345, 60, 60);
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = owner.setFps(30, CURRENT));
        mutation.start();
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));

        owner.close();
        assertTrue("close waits only dispatch, not the host ACK", mutation.isAlive());
        nativeApi.completeMutation(
                MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE);
        waiter.advance();
        mutation.join(1000);
        assertFalse(mutation.isAlive());
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
    }

    @Test
    public void authorizationRevocationAfterAcceptedSendDoesNotRewriteAppliedHostTruth()
            throws Exception {
        FakeNative nativeApi = FakeNative.ready(3, 12345, 60, 60);
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        AtomicBoolean authorized = new AtomicBoolean(true);
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = owner.setFps(30, authorized::get));
        mutation.start();
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));
        authorized.set(false);
        nativeApi.completeMutation(
                MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE);
        waiter.advance();
        mutation.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
    }

    @Test
    public void epochReplacementWinsWhileCommittedMutationIsPending() throws Exception {
        FakeNative epochNative = FakeNative.ready(4, 12345, 60, 60);
        ControlledWaiter epochWaiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings epochOwner = owner(
                epochNative, new FakeScheduler(), ignored -> {}, epochWaiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] epoch = { null };
        Thread epochThread = new Thread(() -> epoch[0] = epochOwner.setFps(30, CURRENT));
        epochThread.start();
        assertTrue(epochWaiter.entered.await(1, TimeUnit.SECONDS));
        epochNative.replaceEpoch(5);
        epochWaiter.advance();
        epochThread.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.STALE, epoch[0]);
    }

    @Test
    public void listenerAndAuthorizationNeverRunUnderOwnerLock() throws Exception {
        Object publicationLock = new Object();
        FakeNative pollNative = FakeNative.querying(6, 12345);
        FakeScheduler scheduler = new FakeScheduler();
        CountDownLatch listenerEntered = new CountDownLatch(1);
        KorriSunshineRuntimeSettings pollOwner = owner(
                pollNative, scheduler,
                ignored -> {
                    listenerEntered.countDown();
                    synchronized (publicationLock) { /* production-shaped Game/coordinator lock */ }
                }, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertTrue(pollOwner.start());
        pollNative.completeQuery();

        Thread pollThread;
        synchronized (publicationLock) {
            pollThread = new Thread(scheduler::runOne);
            pollThread.start();
            assertTrue(listenerEntered.await(1, TimeUnit.SECONDS));
            pollOwner.close();
        }
        pollThread.join(1000);
        assertFalse("teardown cannot deadlock against a listener", pollThread.isAlive());

        FakeNative mutationNative = FakeNative.ready(7, 12345, 60, 60);
        ControlledWaiter mutationWaiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings mutationOwner = owner(
                mutationNative, new FakeScheduler(), ignored -> {}, mutationWaiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        AtomicInteger checks = new AtomicInteger();
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = mutationOwner.setFps(30, () -> {
            checks.incrementAndGet();
            mutationOwner.terminal();
            return true;
        }));
        mutation.start();
        assertTrue(mutationWaiter.entered.await(1, TimeUnit.SECONDS));
        mutationNative.completeMutation(2, 0, 0);
        mutationWaiter.advance();
        mutation.join(1000);
        assertFalse("authorization cannot run under the owner state lock", mutation.isAlive());
        assertEquals(2, checks.get());
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
    }

    @Test
    public void finalAuthorizationCommitCanRevokeBeforeNativeDispatch() {
        FakeNative nativeApi = FakeNative.ready(71, 12345, 60, 60);
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.Authorization revokedAtCommit =
                new KorriSunshineRuntimeSettings.Authorization() {
                    @Override public boolean isCurrent() { return true; }
                    @Override public <T> T commit(
                            java.util.function.Supplier<T> action, T staleResult) {
                        return staleResult;
                    }
                };

        assertEquals(KorriSunshineRuntimeSettings.MutationResult.STALE,
                owner.setFps(30, revokedAtCommit));
        assertEquals(0, nativeApi.mutationCalls);
    }


    @Test
    public void strictFinalCapabilitySnapshotIsReusedAcrossOwnersWithoutQueryBudget() {
        FakeNative nativeApi = FakeNative.ready(72, 12345, 60, 60);
        nativeApi.markStrictFinalCapability(41);
        KorriSunshineRuntimeSettings.ProcessEpochQueryBudget budget =
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget();
        KorriSunshineRuntimeSettings first = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(), budget);
        KorriSunshineRuntimeSettings second = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(), budget);

        assertTrue(first.start());
        assertTrue(second.start());
        assertTrue(nativeApi.queryIds.isEmpty());
    }

    @Test
    public void exactStreamEndedMutationIsFailedBeforeInactiveSessionStaleness() throws Exception {
        FakeNative nativeApi = FakeNative.ready(73, 12345, 60, 60);
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = owner.setFps(30, CURRENT));
        mutation.start();
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));
        nativeApi.completeMutationAndEndStream();
        waiter.advance();
        mutation.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.FAILED, result[0]);
    }

    @Test
    public void requestIdExhaustionNeverPublishesFulfillableControls() throws Exception {
        FakeNative nativeApi = FakeNative.ready(8, 12345, 60, 60);
        nativeApi.wire[20] = Integer.MAX_VALUE;
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertFalse(owner.available(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS));
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.STALE,
                owner.setFps(30, CURRENT));
        assertEquals(0, nativeApi.mutationCalls);

        FakeNative lastIdNative = FakeNative.ready(81, 12345, 60, 60);
        lastIdNative.wire[20] = Integer.MAX_VALUE - 1L;
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings lastIdOwner = owner(
                lastIdNative, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = lastIdOwner.setFps(30, CURRENT));
        mutation.start();
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));
        lastIdNative.completeMutation(2, 0, 0);
        waiter.advance();
        mutation.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
        assertFalse(lastIdOwner.available(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS));
    }

    @Test
    public void fpsUsesMinimumOfCapabilityAndLaunchLimits() {
        FakeNative nativeApi = FakeNative.ready(9, 12345, 120, 60);
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertEquals(60, owner.fpsMax());
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.FAILED,
                owner.setFps(61, CURRENT));
        assertEquals(0, nativeApi.mutationCalls);

        nativeApi.wire[17] = 61;
        KorriSunshineRuntimeSettings invalidCurrent = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertFalse(invalidCurrent.available(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS));
    }

    @Test
    public void sameEpochReplacementCannotExceedThreeQuerySends() {
        KorriSunshineRuntimeSettings.ProcessEpochQueryBudget budget =
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget();
        FakeNative nativeApi = FakeNative.querying(10, 12345);

        FakeScheduler firstScheduler = new FakeScheduler();
        KorriSunshineRuntimeSettings first = owner(
                nativeApi, firstScheduler, ignored -> {}, new ControlledWaiter(), budget);
        assertTrue(first.start());
        assertEquals(1, nativeApi.queryIds.size());
        nativeApi.timeoutQuery();
        firstScheduler.runOne();
        assertEquals(2, nativeApi.queryIds.size());

        FakeScheduler secondScheduler = new FakeScheduler();
        KorriSunshineRuntimeSettings second = owner(
                nativeApi, secondScheduler, ignored -> {}, new ControlledWaiter(), budget);
        assertTrue(second.start()); // Attach to attempt two without charging the budget.
        assertEquals(2, nativeApi.queryIds.size());
        nativeApi.timeoutQuery();
        secondScheduler.runOne();
        assertEquals(3, nativeApi.queryIds.size());

        FakeScheduler thirdScheduler = new FakeScheduler();
        KorriSunshineRuntimeSettings third = owner(
                nativeApi, thirdScheduler, ignored -> {}, new ControlledWaiter(), budget);
        assertTrue(third.start());
        nativeApi.timeoutQuery();
        thirdScheduler.runOne();
        assertEquals(3, nativeApi.queryIds.size());
        assertTrue(third.terminal());
    }

    @Test
    public void olderSameSessionSnapshotCannotRegressNewerState() throws Exception {
        FakeNative nativeApi = FakeNative.ready(101, 12345, 60, 60);
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        long[] olderWire = nativeApi.wire.clone();
        long[] newerWire = nativeApi.wire.clone();
        newerWire[1] = 3;
        newerWire[16] = 13000;
        olderWire[1] = 2;
        olderWire[16] = 12000;
        SunshineRuntimeSettingsSnapshot newer =
                SunshineRuntimeSettingsSnapshot.fromWire(newerWire);
        SunshineRuntimeSettingsSnapshot older =
                SunshineRuntimeSettingsSnapshot.fromWire(olderWire);
        CountDownLatch newerStored = new CountDownLatch(1);
        Thread newThread = new Thread(() -> {
            assertTrue(owner.acceptSnapshot(newer));
            newerStored.countDown();
        });
        Thread oldThread = new Thread(() -> {
            try { assertTrue(newerStored.await(1, TimeUnit.SECONDS)); }
            catch (InterruptedException error) { throw new AssertionError(error); }
            assertFalse(owner.acceptSnapshot(older));
        });
        oldThread.start();
        newThread.start();
        newThread.join(1000);
        oldThread.join(1000);
        assertEquals(13000, owner.bitrate());
    }

    @Test
    public void delayedOldEpochCannotResetNewerProcessBudget() throws Exception {
        KorriSunshineRuntimeSettings.ProcessEpochQueryBudget budget =
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget();
        CountDownLatch releaseOld = new CountDownLatch(1);
        AtomicBoolean oldAcquired = new AtomicBoolean(true);
        Thread old = new Thread(() -> {
            try { releaseOld.await(); }
            catch (InterruptedException error) { throw new AssertionError(error); }
            oldAcquired.set(budget.tryAcquire(110, 3));
        });
        old.start();
        assertTrue(budget.tryAcquire(111, 3));
        releaseOld.countDown();
        old.join(1000);
        assertFalse(oldAcquired.get());
        assertTrue(budget.tryAcquire(111, 3));
        assertTrue(budget.tryAcquire(111, 3));
        assertFalse(budget.tryAcquire(111, 3));
    }

    @Test
    public void closedOldOwnerCallbackCannotRepairReplacement() throws Exception {
        FakeNative firstNative = FakeNative.ready(120, 12345, 60, 60);
        FakeNative secondNative = FakeNative.ready(120, 12345, 60, 60);
        AtomicReference<KorriSunshineRuntimeSettings> current = new AtomicReference<>();
        AtomicInteger repairs = new AtomicInteger();
        CountDownLatch oldCallbackEntered = new CountDownLatch(1);
        CountDownLatch releaseOldCallback = new CountDownLatch(1);
        KorriSunshineRuntimeSettings first = owner(
                firstNative, new FakeScheduler(), source -> {
                    oldCallbackEntered.countDown();
                    try { releaseOldCallback.await(); }
                    catch (InterruptedException error) { throw new AssertionError(error); }
                    if (source == current.get()) repairs.incrementAndGet();
                }, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        current.set(first);
        long[] changedWire = firstNative.wire.clone();
        changedWire[1]++;
        Thread callback = new Thread(() -> first.acceptSnapshot(
                SunshineRuntimeSettingsSnapshot.fromWire(changedWire)));
        callback.start();
        assertTrue(oldCallbackEntered.await(1, TimeUnit.SECONDS));
        first.close();
        KorriSunshineRuntimeSettings replacement = owner(
                secondNative, new FakeScheduler(), source -> {
                    if (source == current.get()) repairs.incrementAndGet();
                }, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        current.set(replacement);
        releaseOldCallback.countDown();
        callback.join(1000);
        assertEquals(0, repairs.get());
    }

    @Test
    public void closeWaitsForActiveQueryDispatchBeforeReplacement() throws Exception {
        FakeNative nativeApi = FakeNative.querying(125, 12345);
        nativeApi.blockQuery = true;
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        Thread start = new Thread(owner::start);
        start.start();
        assertTrue(nativeApi.queryEntered.await(1, TimeUnit.SECONDS));
        Thread close = new Thread(owner::close);
        close.start();
        Thread.sleep(30);
        assertTrue(close.isAlive());
        nativeApi.releaseQuery.countDown();
        close.join(1000);
        start.join(1000);
        assertFalse(close.isAlive());
        assertEquals(1, nativeApi.queryIds.size());

        KorriSunshineRuntimeSettings replacement = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertTrue(replacement.start());
        assertEquals("replacement attaches to the exact in-flight query",
                1, nativeApi.queryIds.size());
    }

    @Test
    public void closeWaitsForActiveDispatchLeaseBeforeSameEpochReplacement() throws Exception {
        FakeNative nativeApi = FakeNative.ready(130, 12345, 60, 60);
        nativeApi.blockMutation = true;
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = owner.setFps(30, CURRENT));
        mutation.start();
        assertTrue(nativeApi.mutationEntered.await(1, TimeUnit.SECONDS));
        Thread close = new Thread(owner::close);
        close.start();
        Thread.sleep(30);
        assertTrue("close must wait for the active JNI dispatch lease", close.isAlive());
        nativeApi.releaseMutation.countDown();
        assertTrue(nativeApi.mutationSent.await(1, TimeUnit.SECONDS));
        close.join(1000);
        assertFalse(close.isAlive());

        KorriSunshineRuntimeSettings replacement = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        assertFalse(replacement.available(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS));
        nativeApi.completeMutation(2, 0, 0);
        waiter.advance();
        mutation.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
    }

    @Test
    public void replacementOwnerPollsFinalCapabilityWithMutationInFlight() {
        FakeNative nativeApi = FakeNative.ready(131, 12345, 60, 60);
        nativeApi.markMutationInFlight(9, MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS);
        FakeScheduler scheduler = new FakeScheduler();
        KorriSunshineRuntimeSettings replacement = owner(
                nativeApi, scheduler, ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());

        assertTrue(replacement.start());
        assertEquals(1, scheduler.pending.size());
        assertEquals(0, nativeApi.queryIds.size());
        nativeApi.completeMutation(
                MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE);
        scheduler.runOne();
        assertTrue(replacement.available(MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS));
    }

    @Test
    public void replacementOwnerReconcilesFinalCapabilitySnapshot() {
        FakeNative nativeApi = FakeNative.ready(132, 12345, 60, 60);
        nativeApi.markReconciliationRequired();
        FakeScheduler scheduler = new FakeScheduler();
        KorriSunshineRuntimeSettings replacement = owner(
                nativeApi, scheduler, ignored -> {}, new ControlledWaiter(),
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());

        assertTrue(replacement.start());
        assertEquals(1, scheduler.pending.size());
        assertEquals(0, nativeApi.queryIds.size());
        scheduler.runOne();
        assertEquals(1, nativeApi.queryIds.size());
    }

    @Test
    public void validatesRestoreAndMultipleAspects() throws Exception {
        assertEquals(480, KorriSunshineRuntimeSettings.safeHeight(854, 1920, 1080));
        assertEquals(720, KorriSunshineRuntimeSettings.safeHeight(1280, 1920, 1080));
        assertEquals(1200, KorriSunshineRuntimeSettings.safeHeight(1600, 2560, 1920));
        assertEquals(-1, KorriSunshineRuntimeSettings.safeHeight(8192, 8193, 8193));

        FakeNative nativeApi = FakeNative.ready(11, 12345, 60, 60);
        ControlledWaiter waiter = new ControlledWaiter();
        KorriSunshineRuntimeSettings owner = owner(
                nativeApi, new FakeScheduler(), ignored -> {}, waiter,
                new KorriSunshineRuntimeSettings.ProcessEpochQueryBudget());
        KorriSunshineRuntimeSettings.MutationResult[] result = { null };
        Thread mutation = new Thread(() -> result[0] = owner.restoreResolution(CURRENT));
        mutation.start();
        assertTrue(waiter.entered.await(1, TimeUnit.SECONDS));
        assertEquals("1920x1080", nativeApi.lastResolution);
        nativeApi.completeMutation(2, 0, 0);
        waiter.advance();
        mutation.join(1000);
        assertEquals(KorriSunshineRuntimeSettings.MutationResult.APPLIED, result[0]);
    }

    private static KorriSunshineRuntimeSettings owner(
            FakeNative nativeApi, FakeScheduler scheduler,
            KorriSunshineRuntimeSettings.Listener listener,
            ControlledWaiter waiter,
            KorriSunshineRuntimeSettings.EpochQueryBudget budget) {
        return new KorriSunshineRuntimeSettings(
                nativeApi, scheduler, listener,
                () -> System.nanoTime() / 1_000_000L,
                waiter,
                budget);
    }

    static final class ControlledWaiter implements KorriSunshineRuntimeSettings.Waiter {
        final CountDownLatch entered = new CountDownLatch(1);
        final Semaphore advances = new Semaphore(0);

        @Override
        public boolean pause(long delayMs, CountDownLatch mutationSignal) throws InterruptedException {
            entered.countDown();
            while (true) {
                if (mutationSignal.getCount() == 0) return true;
                if (advances.tryAcquire(5, TimeUnit.MILLISECONDS)) return false;
            }
        }

        void advance() { advances.release(); }
    }

    static final class FakeScheduler implements KorriSunshineRuntimeSettings.Scheduler {
        final ArrayDeque<Runnable> pending = new ArrayDeque<>();
        @Override public synchronized void postDelayed(Runnable action, long delayMs) { pending.add(action); }
        @Override public synchronized void remove(Runnable action) { pending.removeIf(value -> value == action); }
        void runOne() {
            Runnable action;
            synchronized (this) { action = pending.poll(); }
            if (action != null) action.run();
        }
    }

    static final class FakeNative implements KorriSunshineRuntimeSettings.NativeApi {
        long[] wire;
        final List<Integer> queryIds = new ArrayList<>();
        final CountDownLatch mutationSent = new CountDownLatch(1);
        final CountDownLatch mutationEntered = new CountDownLatch(1);
        final CountDownLatch releaseMutation = new CountDownLatch(1);
        final CountDownLatch queryEntered = new CountDownLatch(1);
        final CountDownLatch releaseQuery = new CountDownLatch(1);
        boolean blockQuery;
        boolean blockMutation;
        int mutationCalls;
        int pendingMutationId;
        String lastResolution;

        static FakeNative ready(long epoch, int bitrate, int maxFps, int launchFps) {
            FakeNative value = new FakeNative();
            value.wire = wire(epoch, bitrate, maxFps, launchFps);
            return value;
        }

        static FakeNative querying(long epoch, int bitrate) {
            FakeNative value = ready(epoch, bitrate, 60, 60);
            value.wire[4] = 0;
            value.wire[20] = 0;
            value.wire[21] = 0;
            return value;
        }

        @Override public synchronized SunshineRuntimeSettingsSnapshot snapshot() {
            return SunshineRuntimeSettingsSnapshot.fromWire(wire.clone());
        }

        @Override public int query(long epoch, int id) {
            queryEntered.countDown();
            if (blockQuery) {
                try { releaseQuery.await(); }
                catch (InterruptedException error) { throw new AssertionError(error); }
            }
            synchronized (this) {
                if (epoch != wire[2]) {
                    return MoonBridge.LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION;
                }
                queryIds.add(id);
                wire = wire.clone();
                wire[1]++;
                wire[20] = id;
                wire[21] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT;
                return 0;
            }
        }

        @Override public int setBitrate(long epoch, int id, int value) {
            return mutate(epoch, id, MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS);
        }

        @Override public int setFps(long epoch, int id, int value) {
            return mutate(epoch, id, MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_FPS);
        }

        @Override public int setResolution(long epoch, int id, int width, int height) {
            lastResolution = width + "x" + height;
            return mutate(epoch, id, MoonBridge.SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION);
        }

        private int mutate(long epoch, int id, int operation) {
            mutationEntered.countDown();
            if (blockMutation) {
                try { releaseMutation.await(); }
                catch (InterruptedException error) { throw new AssertionError(error); }
            }
            synchronized (this) {
                if (epoch != wire[2]) {
                    return MoonBridge.LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION;
                }
                mutationCalls++;
                pendingMutationId = id;
                wire = wire.clone();
                wire[1]++;
                wire[24] = id;
                wire[25] = operation;
                wire[26] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT;
                mutationSent.countDown();
                return 0;
            }
        }

        synchronized void markMutationInFlight(int requestId, int operation) {
            wire = wire.clone();
            pendingMutationId = requestId;
            wire[24] = requestId;
            wire[25] = operation;
            wire[26] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT;
        }

        synchronized void markReconciliationRequired() {
            wire = wire.clone();
            wire[30] = 1;
        }

        synchronized void markStrictFinalCapability(int requestId) {
            wire = wire.clone();
            wire[20] = requestId;
            wire[21] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED;
            wire[22] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
            wire[23] = MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE;
        }

        synchronized void completeMutationAndEndStream() {
            completeMutation(
                    MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED,
                    MoonBridge.SS_RUNTIME_SETTINGS_STATUS_FAILED,
                    MoonBridge.SS_RUNTIME_SETTINGS_REASON_STREAM_ENDED);
            wire[3] = 0;
        }

        synchronized void completeQuery() {
            wire = wire.clone();
            wire[1]++;
            wire[4] = 1;
            wire[5] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
            wire[6] = MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE;
            wire[21] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED;
            wire[22] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
            wire[23] = MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE;
        }

        synchronized void timeoutQuery() {
            wire = wire.clone();
            wire[1]++;
            wire[21] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT;
        }

        synchronized void completeMutation(int outcome, int status, int reason) {
            wire = wire.clone();
            wire[1]++;
            wire[24] = pendingMutationId;
            wire[26] = outcome;
            wire[27] = status;
            wire[28] = reason;
        }

        synchronized void replaceEpoch(long epoch) {
            wire = wire(epoch, 12345, 60, 60);
        }
    }

    private static long[] wire(long epoch, int bitrate, int maxFps, int launchFps) {
        long[] value = new long[SunshineRuntimeSettingsSnapshot.WIRE_LENGTH];
        value[0] = SunshineRuntimeSettingsSnapshot.VERSION;
        value[1] = 1;
        value[2] = epoch;
        value[3] = 1;
        value[4] = 1;
        value[5] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
        value[6] = MoonBridge.SS_RUNTIME_SETTINGS_REASON_NONE;
        value[7] = 14;
        value[9] = 500;
        value[10] = 150000;
        value[11] = maxFps;
        value[12] = bitrate;
        value[13] = launchFps;
        value[14] = 1920;
        value[15] = 1080;
        value[16] = bitrate;
        value[17] = launchFps;
        value[18] = 1920;
        value[19] = 1080;
        value[21] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED;
        value[22] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
        value[26] = MoonBridge.SS_RUNTIME_SETTINGS_OUTCOME_APPLIED;
        value[27] = MoonBridge.SS_RUNTIME_SETTINGS_STATUS_APPLIED;
        return value;
    }
}
