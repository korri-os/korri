package com.limelight.korri.moonlight;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightActionExecutorTest {
    @Test
    public void nativeRuntimeSnapshotOwnsAllSixRuntimePublications() {
        for (KorriMoonlightActionExecutor.Effect effect : new KorriMoonlightActionExecutor.Effect[] {
                KorriMoonlightActionExecutor.Effect.SET_STREAM_BITRATE_KBPS,
                KorriMoonlightActionExecutor.Effect.RESTORE_STREAM_BITRATE,
                KorriMoonlightActionExecutor.Effect.SET_STREAM_FPS,
                KorriMoonlightActionExecutor.Effect.RESTORE_STREAM_FPS,
                KorriMoonlightActionExecutor.Effect.SET_STREAM_WIDTH,
                KorriMoonlightActionExecutor.Effect.RESTORE_STREAM_RESOLUTION,
        }) {
            KorriMoonlightActionExecutor.Request request = effect == KorriMoonlightActionExecutor.Effect.SET_STREAM_BITRATE_KBPS
                    ? KorriMoonlightActionExecutor.Request.range(LAUNCH, effect, 20000)
                    : effect == KorriMoonlightActionExecutor.Effect.SET_STREAM_FPS
                    ? KorriMoonlightActionExecutor.Request.range(LAUNCH, effect, 60)
                    : effect == KorriMoonlightActionExecutor.Effect.SET_STREAM_WIDTH
                    ? KorriMoonlightActionExecutor.Request.range(LAUNCH, effect, 1280)
                    : KorriMoonlightActionExecutor.Request.command(LAUNCH, effect);
            assertFalse(request.needsStatePublication());
        }
    }
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";

    @Test
    public void finalAuthorizationCommitOwnsNonRuntimeSideEffect() {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(
                actions, new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());
        KorriMoonlightActionExecutor.Authorization revokedAtCommit =
                new KorriMoonlightActionExecutor.Authorization() {
                    @Override public boolean isCurrent() { return true; }
                    @Override public <T> T commit(
                            java.util.function.Supplier<T> action, T staleResult) {
                        return staleResult;
                    }
                };

        assertEquals(KorriMoonlightActionExecutor.Outcome.STALE,
                executor.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.DISCONNECT),
                        revokedAtCommit));
        assertTrue(actions.calls.isEmpty());
    }

    @Test
    public void publishesOnlyTheClosedInventoryWithExactCurrentValues() throws Exception {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        actions.fill = true;
        actions.zoom = true;
        actions.mouseMode = "5";
        actions.localCursor = true;
        actions.sharpness = 37;
        actions.threshold = 19;
        actions.faceFlip = true;
        actions.rumble = false;
        actions.pip = true;
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(
                actions, new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());

        JSONObject state = new JSONObject(executor.stateJson(LAUNCH, "executor-generation"));
        assertEquals(LAUNCH, state.getString("launchId"));
        assertEquals("executor-generation", state.getString("generation"));
        assertEquals(24, state.getJSONArray("effects").length());
        assertEquals(true, value(state, "set-fill-mode").getBoolean("value"));
        assertEquals(true, value(state, "set-zoom-mode").getBoolean("value"));
        assertEquals("5", value(state, "set-mouse-mode").getString("value"));
        assertEquals(37, value(state, "set-sgsr-sharpness").getInt("value"));
        assertEquals(19, value(state, "set-sgsr-edge-threshold").getInt("value"));
        assertEquals(20000, value(state, "set-stream-bitrate-kbps").getInt("value"));
        assertEquals(150000, effect(state, "set-stream-bitrate-kbps").getJSONObject("range").getInt("max"));
        assertEquals(120, effect(state, "set-stream-fps").getJSONObject("range").getInt("max"));
        assertEquals(1920, effect(state, "set-stream-width").getJSONObject("range").getInt("max"));
        assertEquals(true, value(state, "set-face-button-flip").getBoolean("value"));
        assertEquals(false, value(state, "set-rumble").getBoolean("value"));
        assertEquals(true, value(state, "set-picture-in-picture").getBoolean("value"));
        assertFalse(state.toString().contains("Activity"));
        assertFalse(state.toString().contains("checkbox_"));
    }

    @Test
    public void isolatesPerEffectUnavailabilityWithoutDroppingHealthyEffects() throws Exception {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        actions.unavailable.add(KorriMoonlightActionExecutor.Effect.SET_MOUSE_MODE);
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(
                actions, new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());

        JSONObject state = new JSONObject(executor.stateJson(LAUNCH, "generation"));
        assertFalse(effect(state, "set-mouse-mode").getBoolean("fulfillable"));
        assertFalse(effect(state, "set-mouse-mode").has("value"));
        assertTrue(effect(state, "disconnect").getBoolean("fulfillable"));
        assertEquals(KorriMoonlightActionExecutor.Outcome.UNAVAILABLE,
                executor.execute(KorriMoonlightActionExecutor.Request.choice(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_MOUSE_MODE, "0")));
    }

    @Test
    public void appliesTypedDesiredValuesIdempotentlyAndRejectsWrongForms() {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(
                actions, new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());

        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_FILL_MODE, true)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_FILL_MODE, true)));
        assertEquals(java.util.Collections.singletonList("fill:true"), actions.calls);

        assertEquals(KorriMoonlightActionExecutor.Outcome.INVALID_VALUE,
                executor.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_FILL_MODE)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.INVALID_VALUE,
                executor.execute(KorriMoonlightActionExecutor.Request.choice(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_MOUSE_MODE, "-1")));
        assertEquals(KorriMoonlightActionExecutor.Outcome.INVALID_VALUE,
                executor.execute(KorriMoonlightActionExecutor.Request.range(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_SGSR_SHARPNESS, 51)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.INVALID_VALUE,
                executor.execute(KorriMoonlightActionExecutor.Request.range(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_SGSR_EDGE_THRESHOLD, 0)));
    }

    @Test
    public void executesEveryCommandToggleChoiceAndRangeAsDistinctGameBehavior() {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(
                actions, new KorriMoonlightActionCoordinatorTest.ImmediateUiDispatcher());

        for (KorriMoonlightActionExecutor.Effect effect : new KorriMoonlightActionExecutor.Effect[] {
                KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD,
                KorriMoonlightActionExecutor.Effect.TOGGLE_FULL_KEYBOARD,
                KorriMoonlightActionExecutor.Effect.SET_LOCAL_CURSOR,
                KorriMoonlightActionExecutor.Effect.ROTATE_SCREEN,
                KorriMoonlightActionExecutor.Effect.TOGGLE_HUD,
                KorriMoonlightActionExecutor.Effect.TOGGLE_FLOATING_MENU,
                KorriMoonlightActionExecutor.Effect.TOGGLE_KEYBOARD_CONTROLLER,
                KorriMoonlightActionExecutor.Effect.SWITCH_TOUCH_SENSITIVITY,
                KorriMoonlightActionExecutor.Effect.DISCONNECT,
                KorriMoonlightActionExecutor.Effect.QUIT_HOST,
        }) {
            assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                    executor.execute(KorriMoonlightActionExecutor.Request.command(LAUNCH, effect)));
        }
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_FILL_MODE, true)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_ZOOM_MODE, true)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.choice(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_MOUSE_MODE, "5")));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.range(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_SGSR_SHARPNESS, 50)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.range(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_SGSR_EDGE_THRESHOLD, 32)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_FACE_BUTTON_FLIP, true)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_RUMBLE, false)));
        assertEquals(KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.toggle(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.SET_PICTURE_IN_PICTURE, true)));

        assertEquals(java.util.Arrays.asList(
                "keyboard", "full-keyboard", "cursor", "rotate", "hud", "floating",
                "keyboard-controller", "touch-sensitivity", "disconnect", "quit",
                "fill:true", "zoom:true", "mouse:5", "sharpness:50", "threshold:32",
                "face:true", "rumble:false", "pip:true"), actions.calls);
    }

    @Test
    public void dispatchesEveryEffectOnTheUiThread() {
        KorriMoonlightActionCoordinatorTest.RecordingActions actions =
                new KorriMoonlightActionCoordinatorTest.RecordingActions();
        QueuedUiDispatcher dispatcher = new QueuedUiDispatcher();
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(actions, dispatcher);

        Thread worker = new Thread(() -> assertEquals(
                KorriMoonlightActionExecutor.Outcome.EXECUTED,
                executor.execute(KorriMoonlightActionExecutor.Request.command(
                        LAUNCH, KorriMoonlightActionExecutor.Effect.DISCONNECT))));
        worker.start();
        dispatcher.drainOnTestUiThread();
        try {
            worker.join(1000);
        } catch (InterruptedException error) {
            throw new AssertionError(error);
        }

        assertEquals(java.util.Collections.singletonList("disconnect"), actions.calls);
        assertTrue(dispatcher.executedOnUiThread);
        assertFalse(worker.isAlive());
    }

    private static JSONObject effect(JSONObject state, String effect) throws Exception {
        JSONArray effects = state.getJSONArray("effects");
        for (int index = 0; index < effects.length(); index++) {
            JSONObject entry = effects.getJSONObject(index);
            if (effect.equals(entry.getString("effect"))) return entry;
        }
        throw new AssertionError("missing effect " + effect);
    }

    private static JSONObject value(JSONObject state, String effect) throws Exception {
        JSONObject entry = effect(state, effect);
        assertTrue(entry.getBoolean("fulfillable"));
        return entry.getJSONObject("value");
    }

    static final class QueuedUiDispatcher
            implements KorriMoonlightActionExecutor.UiDispatcher {
        private final List<Runnable> queued = new ArrayList<>();
        private Thread uiThread;
        boolean executedOnUiThread;

        @Override
        public boolean isUiThread() {
            return Thread.currentThread() == uiThread;
        }

        @Override
        public synchronized void dispatch(Runnable action) {
            queued.add(action);
            notifyAll();
        }

        synchronized void markCurrentUiThread() {
            uiThread = Thread.currentThread();
        }

        synchronized void awaitQueued() {
            long deadline = System.currentTimeMillis() + 1000;
            while (queued.isEmpty() && System.currentTimeMillis() < deadline) {
                try {
                    wait(10);
                } catch (InterruptedException error) {
                    throw new AssertionError(error);
                }
            }
            if (queued.isEmpty()) throw new AssertionError("UI action was not queued");
        }

        synchronized void drainOnTestUiThread() {
            awaitQueued();
            uiThread = Thread.currentThread();
            for (Runnable action : new ArrayList<>(queued)) {
                action.run();
                executedOnUiThread = true;
            }
            queued.clear();
        }
    }
}
