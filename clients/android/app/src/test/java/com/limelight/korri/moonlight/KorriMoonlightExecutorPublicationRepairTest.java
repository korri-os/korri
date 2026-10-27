package com.limelight.korri.moonlight;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightExecutorPublicationRepairTest {
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";

    @Test
    public void initialPublicationFailureKeepsExactExecutorAndRetriesFreshGeneration() {
        ScriptedPublication publication = new ScriptedPublication();
        publication.failNext = 1;
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        QueueScheduler scheduler = new QueueScheduler();
        AtomicBoolean current = new AtomicBoolean(true);
        KorriMoonlightActionExecutor executor = executor();
        List<String> generations = new ArrayList<>();
        KorriMoonlightExecutorPublicationRepair repair =
                new KorriMoonlightExecutorPublicationRepair(
                        coordinator, scheduler, current::get, generations::add,
                        LAUNCH, executor);

        repair.start();
        assertNull(repair.generation());
        assertEquals(1, scheduler.pending.size());
        scheduler.runOne();

        String generation = repair.generation();
        assertNotNull(generation);
        assertEquals(generation, generations.get(generations.size() - 1));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
    }

    @Test
    public void terminalSnapshotRepublishFailureRepairsAndOldGenerationIsStale() {
        ScriptedPublication publication = new ScriptedPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        QueueScheduler scheduler = new QueueScheduler();
        KorriMoonlightActionExecutor executor = executor();
        List<String> generations = new ArrayList<>();
        KorriMoonlightExecutorPublicationRepair repair =
                new KorriMoonlightExecutorPublicationRepair(
                        coordinator, scheduler, () -> true, generations::add,
                        LAUNCH, executor);

        repair.start();
        String oldGeneration = repair.generation();
        publication.failNext = 1;
        repair.snapshotChanged();
        String repairedGeneration = repair.generation();

        assertNotNull(repairedGeneration);
        assertNotEquals(oldGeneration, repairedGeneration);
        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, oldGeneration,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, repairedGeneration,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
    }

    @Test
    public void delayedRetryIsExactLaunchGuarded() {
        ScriptedPublication publication = new ScriptedPublication();
        publication.failNext = 3;
        QueueScheduler scheduler = new QueueScheduler();
        AtomicBoolean current = new AtomicBoolean(true);
        KorriMoonlightExecutorPublicationRepair repair =
                new KorriMoonlightExecutorPublicationRepair(
                        new KorriMoonlightActionCoordinator(publication),
                        scheduler, current::get, generation -> {}, LAUNCH, executor());
        repair.start();
        current.set(false);
        scheduler.runOne();
        assertNull(repair.generation());
        assertTrue(scheduler.pending.isEmpty());
    }

    @Test
    public void closeDuringPausedStateMaterializationPreventsFinalPublish() throws Exception {
        ScriptedPublication publication = new ScriptedPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        actions.blockNextStateRead();
        KorriMoonlightActionExecutor executor = executor(actions);
        List<String> generations = new ArrayList<>();
        KorriMoonlightExecutorPublicationRepair repair =
                new KorriMoonlightExecutorPublicationRepair(
                        coordinator, new QueueScheduler(), () -> true, generations::add,
                        LAUNCH, executor);

        Thread start = new Thread(repair::start);
        start.start();
        actions.awaitStateRead();
        repair.close();
        actions.releaseStateRead();
        start.join(1000);

        assertFalse(start.isAlive());
        assertNull(repair.generation());
        assertTrue(publication.states.isEmpty());
        assertFalse(coordinator.unregisterExactExecutor(LAUNCH, executor));
        assertEquals(java.util.Collections.singletonList(null), generations);
    }

    @Test
    public void closeWaitsForEnteredInstallThenRemovesExactOwner() throws Exception {
        ScriptedPublication publication = new ScriptedPublication();
        publication.blockNextPublish();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionExecutor executor = executor();
        KorriMoonlightExecutorPublicationRepair repair =
                new KorriMoonlightExecutorPublicationRepair(
                        coordinator, new QueueScheduler(), () -> true, generation -> {},
                        LAUNCH, executor);

        Thread start = new Thread(repair::start);
        start.start();
        publication.awaitPublish();
        Thread close = new Thread(repair::close);
        close.start();
        Thread.sleep(25);
        assertTrue("close must wait for the short final install", close.isAlive());
        publication.releasePublish();
        start.join(1000);
        close.join(1000);

        assertFalse(start.isAlive());
        assertFalse(close.isAlive());
        assertNull(repair.generation());
        assertEquals(1, publication.states.size());
        assertEquals(1, publication.cleared.size());
        assertFalse(coordinator.unregisterExactExecutor(LAUNCH, executor));
    }

    @Test
    public void closedOldSameLaunchRepairCannotWinAgainstReplacement() throws Exception {
        ScriptedPublication publication = new ScriptedPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionCoordinatorTest.RecordingActions oldActions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        oldActions.blockNextStateRead();
        KorriMoonlightActionExecutor oldExecutor = executor(oldActions);
        KorriMoonlightExecutorPublicationRepair oldRepair =
                new KorriMoonlightExecutorPublicationRepair(
                        coordinator, new QueueScheduler(), () -> true, generation -> {},
                        LAUNCH, oldExecutor);
        Thread oldStart = new Thread(oldRepair::start);
        oldStart.start();
        oldActions.awaitStateRead();

        oldRepair.close();
        KorriMoonlightActionCoordinatorTest.RecordingActions replacementActions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        KorriMoonlightActionExecutor replacement = executor(replacementActions);
        String replacementGeneration = coordinator.register(LAUNCH, replacement);
        oldActions.releaseStateRead();
        oldStart.join(1000);

        assertFalse(oldStart.isAlive());
        assertNotNull(replacementGeneration);
        assertEquals(1, publication.states.size());
        assertTrue(publication.cleared.isEmpty());
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, replacementGeneration,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
        assertEquals(java.util.Collections.singletonList("keyboard"), replacementActions.calls);
        assertFalse(coordinator.unregisterExactExecutor(LAUNCH, oldExecutor));
    }

    @Test
    public void delayedOldRepairCannotReplaceNewSameLaunchExecutor() throws Exception {
        ScriptedPublication publication = new ScriptedPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionExecutor oldExecutor = executor();
        KorriMoonlightActionExecutor newExecutor = executor();
        CountDownLatch oldMayRegister = new CountDownLatch(1);
        AtomicReference<String> oldGeneration = new AtomicReference<>();
        Thread oldAttempt = new Thread(() -> {
            try { oldMayRegister.await(); }
            catch (InterruptedException error) { throw new AssertionError(error); }
            oldGeneration.set(coordinator.registerIfAbsent(LAUNCH, oldExecutor));
        });
        oldAttempt.start();

        String newGeneration = coordinator.register(LAUNCH, newExecutor);
        oldMayRegister.countDown();
        oldAttempt.join(1000);

        assertNull(oldGeneration.get());
        assertTrue(coordinator.republishExact(LAUNCH, newGeneration, newExecutor));
        assertFalse(coordinator.republishExact(LAUNCH, newGeneration, oldExecutor));
        assertFalse(coordinator.unregister(LAUNCH, newGeneration, oldExecutor));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, newGeneration,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
    }

    private static KorriMoonlightActionExecutor executor() {
        return executor(new KorriMoonlightActionCoordinatorTest.RecordingActions());
    }

    private static KorriMoonlightActionExecutor executor(
            KorriMoonlightActionCoordinatorTest.RecordingActions actions) {
        return new KorriMoonlightActionExecutor(
                actions,
                new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());
    }

    private static final class QueueScheduler
            implements KorriMoonlightExecutorPublicationRepair.Scheduler {
        final ArrayDeque<Runnable> pending = new ArrayDeque<>();
        @Override public void postDelayed(Runnable action, long delayMs) { pending.add(action); }
        void runOne() { Runnable action = pending.poll(); if (action != null) action.run(); }
    }

    private static final class ScriptedPublication
            implements KorriMoonlightActionCoordinator.Publication {
        int failNext;
        final List<String> states = java.util.Collections.synchronizedList(new ArrayList<>());
        final List<String> cleared = java.util.Collections.synchronizedList(new ArrayList<>());
        private CountDownLatch publishEntered;
        private CountDownLatch publishRelease;

        synchronized void blockNextPublish() {
            publishEntered = new CountDownLatch(1);
            publishRelease = new CountDownLatch(1);
        }

        void awaitPublish() throws InterruptedException {
            assertTrue("final publish did not start", publishEntered.await(1, TimeUnit.SECONDS));
        }

        void releasePublish() { publishRelease.countDown(); }

        @Override
        public boolean publish(String stateJson) {
            CountDownLatch entered;
            CountDownLatch release;
            synchronized (this) {
                entered = publishEntered;
                release = publishRelease;
                publishEntered = null;
                if (failNext > 0) {
                    failNext--;
                    return false;
                }
            }
            if (entered != null) {
                entered.countDown();
                try {
                    if (!release.await(1, TimeUnit.SECONDS)) {
                        throw new AssertionError("final publish was not released");
                    }
                } catch (InterruptedException error) {
                    throw new AssertionError(error);
                }
            }
            if (release != null) {
                synchronized (this) {
                    if (publishRelease == release) publishRelease = null;
                }
            }
            states.add(stateJson);
            return true;
        }

        @Override public boolean clear(String launchId, String generation) {
            cleared.add(launchId + ":" + generation);
            return true;
        }
    }
}
