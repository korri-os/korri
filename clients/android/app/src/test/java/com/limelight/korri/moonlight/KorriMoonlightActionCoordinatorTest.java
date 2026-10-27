package com.limelight.korri.moonlight;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightActionCoordinatorTest {
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";

    @Test
    public void unavailableBeforeRegistrationAndAfterIdentityClear() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionExecutor executor = executor(new RecordingActions());

        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, "not-registered",
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        String generation = coordinator.register(LAUNCH, executor);
        assertTrue(generation != null);
        assertTrue(coordinator.unregister(LAUNCH, generation, executor));
        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        assertEquals(LAUNCH + ":" + generation, publication.cleared.get(0));
    }

    @Test
    public void sameLaunchReplacementGetsNewGenerationAndRejectsOldTerminalInstructions()
            throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions first = new RecordingActions();
        RecordingActions replacement = new RecordingActions();
        KorriMoonlightActionExecutor executorA = executor(first);
        KorriMoonlightActionExecutor executorB = executor(replacement);

        String generationA = coordinator.register(LAUNCH, executorA);
        String generationB = coordinator.register(LAUNCH, executorB);
        assertFalse(generationA.equals(generationB));
        assertFalse(coordinator.unregister(LAUNCH, generationA, executorA));
        for (KorriMoonlightActionExecutor.Effect terminal : new KorriMoonlightActionExecutor.Effect[] {
                KorriMoonlightActionExecutor.Effect.DISCONNECT,
                KorriMoonlightActionExecutor.Effect.QUIT_HOST,
        }) {
            assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                    coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                            LAUNCH, generationA, terminal)));
        }
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generationB,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));

        assertTrue(first.calls.isEmpty());
        assertEquals(java.util.Collections.singletonList("keyboard"), replacement.calls);
        assertTrue(publication.cleared.isEmpty());
        assertEquals(generationA,
                new JSONObject(publication.states.get(0)).getString("generation"));
        assertEquals(generationB,
                new JSONObject(publication.states.get(1)).getString("generation"));
    }

    @Test
    public void queuedOldTerminalEffectRechecksGenerationOnUiThread() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions first = new RecordingActions();
        KorriMoonlightActionExecutorTest.QueuedUiDispatcher queued =
                new KorriMoonlightActionExecutorTest.QueuedUiDispatcher();
        KorriMoonlightActionExecutor executorA =
                new KorriMoonlightActionExecutor(first, queued);
        queued.markCurrentUiThread();
        String generationA = coordinator.register(LAUNCH, executorA);
        RecordingActions replacement = new RecordingActions();
        KorriMoonlightActionExecutor executorB = executor(replacement);

        final KorriMoonlightActionExecutor.Outcome[] outcome = { null };
        Thread worker = new Thread(() -> outcome[0] = coordinator.execute(
                KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generationA,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        worker.start();
        queued.awaitQueued();
        String generationB = coordinator.register(LAUNCH, executorB);
        assertFalse(generationA.equals(generationB));
        queued.drainOnTestUiThread();
        try {
            worker.join(1000);
        } catch (InterruptedException error) {
            throw new AssertionError(error);
        }

        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE, outcome[0]);
        assertTrue(first.calls.isEmpty());
        assertTrue(replacement.calls.isEmpty());
    }

    @Test
    public void callerCommitAndExactGenerationOwnTheFinalSideEffectTogether() {
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(new RecordingPublication());
        RecordingActions actions = new RecordingActions();
        String generation = coordinator.register(LAUNCH, executor(actions));
        KorriMoonlightActionExecutor.Authorization revokedAtCommit =
                new KorriMoonlightActionExecutor.Authorization() {
                    @Override public boolean isCurrent() { return true; }
                    @Override public <T> T commit(
                            java.util.function.Supplier<T> action, T staleResult) {
                        return staleResult;
                    }
                };

        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT),
                        revokedAtCommit));
        assertTrue(actions.calls.isEmpty());
    }

    @Test
    public void stateRepublishFailureClearsExactGenerationAndInvalidatesExecutor() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions actions = new RecordingActions();
        String generation = coordinator.register(LAUNCH, executor(actions));
        publication.failPublish = true;

        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.SET_FILL_MODE, true)));
        assertEquals(java.util.Collections.singletonList("fill:true"), actions.calls);
        assertEquals(java.util.Collections.singletonList(LAUNCH + ":" + generation),
                publication.cleared);
        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
    }

    @Test
    public void overlayRepublishFailureClearsExactGenerationAndInvalidatesExecutor() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        String generation = coordinator.register(LAUNCH, executor(new RecordingActions()));
        publication.failPublish = true;

        assertFalse(coordinator.republish(LAUNCH));
        assertEquals(java.util.Collections.singletonList(LAUNCH + ":" + generation),
                publication.cleared);
        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
    }

    @Test
    public void failedSameLaunchReplacementClearsPreviousGenerationAndLeavesUnavailable() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions first = new RecordingActions();
        String generationA = coordinator.register(LAUNCH, executor(first));
        publication.failPublish = true;

        assertNull(coordinator.register(LAUNCH, executor(new RecordingActions())));
        assertEquals(java.util.Collections.singletonList(LAUNCH + ":" + generationA),
                publication.cleared);
        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generationA,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
        assertTrue(first.calls.isEmpty());
    }

    @Test
    public void transientRepublishFailureRepairsWithFreshGenerationAndRejectsOld() {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator = new KorriMoonlightActionCoordinator(publication);
        KorriMoonlightActionExecutor executor = executor(new RecordingActions());
        String oldGeneration = coordinator.register(LAUNCH, executor);
        publication.failPublish = true;
        assertFalse(coordinator.republish(LAUNCH));
        publication.failPublish = false;
        String repairedGeneration = coordinator.register(LAUNCH, executor);
        assertTrue(repairedGeneration != null);
        assertFalse(oldGeneration.equals(repairedGeneration));
        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, oldGeneration, KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, repairedGeneration, KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
    }

    @Test
    public void overlayOpeningRepublishesExternallyMutatedLiveValues() throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions actions = new RecordingActions();
        coordinator.register(LAUNCH, executor(actions));

        actions.fill = true;
        assertTrue(coordinator.republish(LAUNCH));

        JSONObject state = new JSONObject(publication.states.get(1));
        JSONObject fill = state.getJSONArray("effects").getJSONObject(4);
        assertEquals("set-fill-mode", fill.getString("effect"));
        assertTrue(fill.getJSONObject("value").getBoolean("value"));
    }

    @Test
    public void queuedUiCommitCompletesWhileBackgroundRepublishWaitsForUi() throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions actions = new RecordingActions();
        ControlledUiDispatcher ui = new ControlledUiDispatcher();
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(actions, ui);
        String generation = coordinator.register(LAUNCH, executor);
        ui.transferToPausedWorker();

        final KorriMoonlightActionExecutor.Outcome[] actionOutcome = { null };
        final boolean[] republished = { false };
        Thread action = new Thread(() -> actionOutcome[0] = coordinator.execute(
                KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, generation,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
        action.start();
        ui.awaitQueued(1);
        Thread republish = new Thread(() -> republished[0] = coordinator.republishExact(
                LAUNCH, generation, executor));
        republish.start();
        ui.awaitQueued(2);

        ui.release();
        action.join(1000);
        republish.join(1000);
        ui.close();

        assertFalse("queued exact action deadlocked on coordinator", action.isAlive());
        assertFalse("republish did not complete after UI state materialization", republish.isAlive());
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED, actionOutcome[0]);
        assertTrue(republished[0]);
        assertEquals(java.util.Collections.singletonList("keyboard"), actions.calls);
        assertEquals(2, publication.states.size());
        assertTrue(publication.cleared.isEmpty());
    }

    @Test
    public void staleReplacementMaterializationCannotReplaceOrClearNewerOwner() throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        String originalGeneration = coordinator.register(
                LAUNCH, executor(new RecordingActions()));
        RecordingActions delayedActions = new RecordingActions();
        delayedActions.blockNextStateRead();
        KorriMoonlightActionExecutor delayed = executor(delayedActions);
        final String[] delayedGeneration = { "not-finished" };
        Thread delayedRegistration = new Thread(() -> delayedGeneration[0] =
                coordinator.register(LAUNCH, delayed));
        delayedRegistration.start();
        delayedActions.awaitStateRead();

        RecordingActions replacementActions = new RecordingActions();
        KorriMoonlightActionExecutor replacement = executor(replacementActions);
        String replacementGeneration = coordinator.register(LAUNCH, replacement);
        delayedActions.releaseStateRead();
        delayedRegistration.join(1000);

        assertFalse(delayedRegistration.isAlive());
        assertNull(delayedGeneration[0]);
        assertFalse(originalGeneration.equals(replacementGeneration));
        assertEquals(2, publication.states.size());
        assertTrue(publication.cleared.isEmpty());
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, replacementGeneration,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
        assertEquals(java.util.Collections.singletonList("keyboard"), replacementActions.calls);
    }

    @Test
    public void staleRegisterIfAbsentMaterializationCannotReplaceAnotherOwner() throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions delayedActions = new RecordingActions();
        delayedActions.blockNextStateRead();
        KorriMoonlightActionExecutor delayed = executor(delayedActions);
        final String[] delayedGeneration = { "not-finished" };
        Thread delayedRegistration = new Thread(() -> delayedGeneration[0] =
                coordinator.registerIfAbsent(LAUNCH, delayed));
        delayedRegistration.start();
        delayedActions.awaitStateRead();

        KorriMoonlightActionExecutor replacement = executor(new RecordingActions());
        String replacementGeneration = coordinator.registerIfAbsent(LAUNCH, replacement);
        delayedActions.releaseStateRead();
        delayedRegistration.join(1000);

        assertFalse(delayedRegistration.isAlive());
        assertNull(delayedGeneration[0]);
        assertTrue(replacementGeneration != null);
        assertEquals(1, publication.states.size());
        assertTrue(publication.cleared.isEmpty());
    }

    @Test
    public void staleRepublishMaterializationCannotPublishOrInvalidateReplacement() throws Exception {
        RecordingPublication publication = new RecordingPublication();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(publication);
        RecordingActions originalActions = new RecordingActions();
        KorriMoonlightActionExecutor original = executor(originalActions);
        String originalGeneration = coordinator.register(LAUNCH, original);
        originalActions.blockNextStateRead();
        final boolean[] republished = { true };
        Thread delayedRepublish = new Thread(() -> republished[0] = coordinator.republishExact(
                LAUNCH, originalGeneration, original));
        delayedRepublish.start();
        originalActions.awaitStateRead();

        RecordingActions replacementActions = new RecordingActions();
        KorriMoonlightActionExecutor replacement = executor(replacementActions);
        String replacementGeneration = coordinator.register(LAUNCH, replacement);
        originalActions.releaseStateRead();
        delayedRepublish.join(1000);

        assertFalse(delayedRepublish.isAlive());
        assertFalse(republished[0]);
        assertEquals(2, publication.states.size());
        assertTrue(publication.cleared.isEmpty());
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, replacementGeneration,
                        KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD)));
        assertEquals(java.util.Collections.singletonList("keyboard"), replacementActions.calls);
    }

    @Test
    public void foreignLaunchCannotReachCurrentExecutor() {
        RecordingActions actions = new RecordingActions();
        KorriMoonlightActionCoordinator coordinator =
                new KorriMoonlightActionCoordinator(new RecordingPublication());
        String generation = coordinator.register(LAUNCH, executor(actions));

        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                coordinator.execute(KorriMoonlightActionExecutor.Request.command(
                        "fedcba9876543210fedcba9876543210", generation,
                        KorriMoonlightActionExecutor.Effect.DISCONNECT)));
        assertTrue(actions.calls.isEmpty());
    }

    private static KorriMoonlightActionExecutor executor(RecordingActions actions) {
        return new KorriMoonlightActionExecutor(actions, new ImmediateUiDispatcher());
    }

    private static final class RecordingPublication
            implements KorriMoonlightActionCoordinator.Publication {
        final List<String> states = new ArrayList<>();
        final List<String> cleared = new ArrayList<>();
        boolean failPublish;

        @Override
        public boolean publish(String stateJson) {
            states.add(stateJson);
            return !failPublish;
        }

        @Override
        public boolean clear(String launchId, String generation) {
            cleared.add(launchId + ":" + generation);
            return true;
        }
    }

    static final class ControlledUiDispatcher
            implements KorriMoonlightActionExecutor.UiDispatcher, AutoCloseable {
        private static final Runnable STOP = () -> { };
        private final BlockingQueue<Runnable> queued = new LinkedBlockingQueue<>();
        private final CountDownLatch workerStarted = new CountDownLatch(1);
        private final CountDownLatch release = new CountDownLatch(1);
        private volatile Thread uiThread = Thread.currentThread();
        private Thread worker;

        @Override public boolean isUiThread() { return Thread.currentThread() == uiThread; }
        @Override public void dispatch(Runnable action) { queued.add(action); }

        void transferToPausedWorker() throws InterruptedException {
            worker = new Thread(() -> {
                uiThread = Thread.currentThread();
                workerStarted.countDown();
                try {
                    release.await();
                    while (true) {
                        Runnable action = queued.take();
                        if (action == STOP) return;
                        action.run();
                    }
                } catch (InterruptedException ignored) {
                    Thread.currentThread().interrupt();
                }
            });
            worker.start();
            assertTrue(workerStarted.await(1, TimeUnit.SECONDS));
        }

        void awaitQueued(int count) throws InterruptedException {
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(1);
            while (queued.size() < count && System.nanoTime() < deadline) {
                Thread.sleep(5);
            }
            assertTrue("expected " + count + " queued UI actions, got " + queued.size(),
                    queued.size() >= count);
        }

        void release() { release.countDown(); }

        @Override public void close() throws InterruptedException {
            release.countDown();
            queued.add(STOP);
            if (worker != null) worker.join(1000);
        }
    }

    static final class ImmediateUiDispatcher
            implements KorriMoonlightActionExecutor.UiDispatcher {
        @Override
        public boolean isUiThread() {
            return true;
        }

        @Override
        public void dispatch(Runnable action) {
            action.run();
        }
    }

    static final class RecordingActions implements KorriMoonlightActionExecutor.Actions {
        final List<String> calls = new ArrayList<>();
        boolean fill;
        boolean zoom;
        String mouseMode = "0";
        boolean localCursor;
        int sharpness = 20;
        int threshold = 8;
        boolean faceFlip;
        boolean rumble = true;
        boolean pip;
        final java.util.Set<KorriMoonlightActionExecutor.Effect> unavailable =
                new java.util.HashSet<>();
        private CountDownLatch stateReadEntered;
        private CountDownLatch stateReadRelease;
        private boolean blockNextStateRead;

        synchronized void blockNextStateRead() {
            stateReadEntered = new CountDownLatch(1);
            stateReadRelease = new CountDownLatch(1);
            blockNextStateRead = true;
        }

        void awaitStateRead() throws InterruptedException {
            assertTrue("state materialization did not start",
                    stateReadEntered.await(1, TimeUnit.SECONDS));
        }

        void releaseStateRead() {
            stateReadRelease.countDown();
        }

        @Override public boolean available(KorriMoonlightActionExecutor.Effect effect) {
            CountDownLatch release = null;
            synchronized (this) {
                if (blockNextStateRead) {
                    blockNextStateRead = false;
                    stateReadEntered.countDown();
                    release = stateReadRelease;
                }
            }
            if (release != null) {
                try {
                    if (!release.await(1, TimeUnit.SECONDS)) {
                        throw new AssertionError("state materialization was not released");
                    }
                } catch (InterruptedException error) {
                    throw new AssertionError(error);
                }
            }
            return !unavailable.contains(effect);
        }
        @Override public boolean fillMode() { return fill; }
        @Override public void setFillMode(boolean value) { fill = value; calls.add("fill:" + value); }
        @Override public boolean zoomMode() { return zoom; }
        @Override public void setZoomMode(boolean value) { zoom = value; calls.add("zoom:" + value); }
        @Override public String mouseMode() { return mouseMode; }
        @Override public void setMouseMode(String value) { mouseMode = value; calls.add("mouse:" + value); }
        @Override public boolean localCursor() { return localCursor; }
        @Override public void toggleLocalCursor() { localCursor = !localCursor; calls.add("cursor"); }
        @Override public int sgsrSharpness() { return sharpness; }
        @Override public void setSgsrSharpness(int value) { sharpness = value; calls.add("sharpness:" + value); }
        @Override public int sgsrEdgeThreshold() { return threshold; }
        @Override public void setSgsrEdgeThreshold(int value) { threshold = value; calls.add("threshold:" + value); }
        @Override public boolean faceButtonFlip() { return faceFlip; }
        @Override public void setFaceButtonFlip(boolean value) { faceFlip = value; calls.add("face:" + value); }
        @Override public boolean rumble() { return rumble; }
        @Override public void setRumble(boolean value) { rumble = value; calls.add("rumble:" + value); }
        @Override public boolean pictureInPicture() { return pip; }
        @Override public void setPictureInPicture(boolean value) { pip = value; calls.add("pip:" + value); }
        @Override public void disconnect() { calls.add("disconnect"); }
        @Override public void quitHost() { calls.add("quit"); }
        @Override public void toggleKeyboard() { calls.add("keyboard"); }
        @Override public void toggleFullKeyboard() { calls.add("full-keyboard"); }
        @Override public void rotateScreen() { calls.add("rotate"); }
        @Override public void toggleHud() { calls.add("hud"); }
        @Override public void toggleFloatingMenu() { calls.add("floating"); }
        @Override public void toggleKeyboardController() { calls.add("keyboard-controller"); }
        @Override public void switchTouchSensitivity() { calls.add("touch-sensitivity"); }
        @Override public int streamBitrateKbps() { return 20000; }
        @Override public int streamBitrateMinKbps() { return 500; }
        @Override public int streamBitrateMaxKbps() { return 150000; }
        @Override public KorriSunshineRuntimeSettings.MutationResult setStreamBitrateKbps(int value, KorriMoonlightActionExecutor.Authorization authorization) { calls.add("bitrate:" + value); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
        @Override public KorriSunshineRuntimeSettings.MutationResult restoreStreamBitrate(KorriMoonlightActionExecutor.Authorization authorization) { calls.add("restore-bitrate"); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
        @Override public int streamFps() { return 60; }
        @Override public int streamFpsMax() { return 120; }
        @Override public KorriSunshineRuntimeSettings.MutationResult setStreamFps(int value, KorriMoonlightActionExecutor.Authorization authorization) { calls.add("fps:" + value); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
        @Override public KorriSunshineRuntimeSettings.MutationResult restoreStreamFps(KorriMoonlightActionExecutor.Authorization authorization) { calls.add("restore-fps"); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
        @Override public int streamWidth() { return 1920; }
        @Override public int streamWidthMax() { return 1920; }
        @Override public KorriSunshineRuntimeSettings.MutationResult setStreamWidth(int value, KorriMoonlightActionExecutor.Authorization authorization) { calls.add("width:" + value); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
        @Override public KorriSunshineRuntimeSettings.MutationResult restoreStreamResolution(KorriMoonlightActionExecutor.Authorization authorization) { calls.add("restore-resolution"); return authorization.isCurrent() ? KorriSunshineRuntimeSettings.MutationResult.APPLIED : KorriSunshineRuntimeSettings.MutationResult.STALE; }
    }
}
