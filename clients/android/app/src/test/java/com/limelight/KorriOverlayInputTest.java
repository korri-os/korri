package com.limelight;

import android.graphics.SurfaceTexture;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceView;
import com.limelight.ui.StreamContainer;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;
import java.nio.file.Files;
import java.nio.file.Path;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriOverlayInputTest {
    @Test public void preImeAndActivityKeysReachOverlayAcrossSeveralLifetimes() {
        Game game = Robolectric.buildActivity(Game.class).get();
        StreamContainer stream = new StreamContainer(game, null);
        stream.setInputCallbacks(game);
        // Do not start a decoder or network connection. Any leaked input would
        // enter the real Game stream path and fail rather than use a stand-in.
        for (int cycle = 0; cycle < 5; cycle++) {
            Surface surface = new Surface(new SurfaceTexture(0));
            KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
            ReflectionHelpers.setField(game, "surfaceOverlay", new SurfaceView(game));
            assertTrue(stream.onKeyPreIme(KeyEvent.KEYCODE_DPAD_DOWN,
                    new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_DOWN)));
            assertTrue(stream.onKeyPreIme(KeyEvent.KEYCODE_DPAD_DOWN,
                    new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_DPAD_DOWN)));
            int[] keys = KorriSurfaceHost.takeKeys();
            assertEquals(6, keys.length);
            assertEquals(KeyEvent.KEYCODE_DPAD_DOWN, keys[1]);
            assertEquals(1, keys[2]);
            assertEquals(0, keys[5]);
            assertTrue(game.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_A)));
            assertEquals(KeyEvent.KEYCODE_BUTTON_A, KorriSurfaceHost.takeKeys()[1]);
            MotionEvent motion = MotionEvent.obtain(0, 0, MotionEvent.ACTION_MOVE, 40, 30, 0);
            assertTrue(game.dispatchGenericMotionEvent(motion));
            assertTrue(game.dispatchTouchEvent(motion));
            assertTrue(game.handleMotionEvent(stream, motion));
            motion.recycle();
            KorriSurfaceHost.detach(attachment);
            ReflectionHelpers.setField(game, "surfaceOverlay", null);
            assertArrayEquals(new int[0], KorriSurfaceHost.takeKeys());
            surface.release();
        }
    }

    @Test public void activityRoutesTheWholeTouchGestureToSlintInSurfaceCoordinates() {
        Game game = Robolectric.buildActivity(Game.class).get();
        SurfaceView view = new SurfaceView(game);
        view.layout(10, 20, 650, 500);
        ReflectionHelpers.setField(game, "surfaceOverlay", view);
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 2, () -> fail("Unexpected failure"));
        try {
            for (int action : new int[] { 0, 2, 1, 3 }) {
                MotionEvent event = MotionEvent.obtain(0, 0, action, 120, 240, 0);
                event.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
                assertTrue(game.dispatchTouchEvent(event));
                int[] origin = new int[2];
                view.getLocationInWindow(origin);
                assertArrayEquals(new float[] { action, 0, 120 - origin[0], 240 - origin[1] }, KorriSurfaceHost.takePointers(), 0);
                assertArrayEquals(new int[0], KorriSurfaceHost.takeAxes());
                event.recycle();
            }
        } finally {
            KorriSurfaceHost.detach(attachment);
            surface.release();
        }
    }

    @Test public void overlayLeavesSystemVolumeAndPowerKeysToAndroidWhileGameplayStaysHeld() {
        Game game = Robolectric.buildActivity(Game.class).get();
        com.limelight.nvstream.NvConnection connection = new com.limelight.nvstream.NvConnection(
                game, new com.limelight.nvstream.http.ComputerDetails.AddressTuple("127.0.0.1", 47989),
                47984, "test", null, null, null, com.limelight.nvstream.NvConnection.AppLaunchPolicy.EXACT);
        ReflectionHelpers.setField(game, "conn", connection);
        ReflectionHelpers.setField(game, "korriViewerRequested", true);
        ReflectionHelpers.setField(game, "surfaceOverlay", new SurfaceView(game));
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
        connection.setInputHeld(true);
        try {
            for (int code : new int[] { KeyEvent.KEYCODE_VOLUME_UP, KeyEvent.KEYCODE_VOLUME_DOWN,
                    KeyEvent.KEYCODE_VOLUME_MUTE, KeyEvent.KEYCODE_POWER }) {
                // These are the pre-IME/Activity callbacks before Android's own
                // default handling. Returning false gives the system its key.
                assertFalse(game.handleKeyDown(new KeyEvent(KeyEvent.ACTION_DOWN, code)));
                assertFalse(game.handleKeyUp(new KeyEvent(KeyEvent.ACTION_UP, code)));
                assertArrayEquals(new int[0], KorriSurfaceHost.takeKeys());
                assertTrue(ReflectionHelpers.<Boolean>getField(connection, "inputHeld"));
            }
            assertTrue(game.handleKeyDown(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_DOWN)));
            assertTrue(game.handleKeyUp(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_DPAD_DOWN)));
            assertEquals(6, KorriSurfaceHost.takeKeys().length);
            assertTrue(ReflectionHelpers.<Boolean>getField(connection, "inputHeld"));
        } finally {
            KorriSurfaceHost.detach(attachment);
            surface.release();
        }
    }

    @Test public void korriGuideSendsOneResumePairOnReleaseAcrossPreImeAndActivityDispatch() {
        Game game = Robolectric.buildActivity(Game.class).get();
        ReflectionHelpers.setField(game, "korriViewerRequested", true);
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
        ReflectionHelpers.setField(game, "surfaceOverlay", new SurfaceView(game));
        try {
            assertTrue(game.handleKeyDown(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_MODE)));
            assertArrayEquals(new int[0], KorriSurfaceHost.takeKeys());
            assertTrue(game.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_MODE)));
            int[] keys = KorriSurfaceHost.takeKeys();
            assertEquals(6, keys.length);
            assertEquals(KeyEvent.KEYCODE_MENU, keys[1]);
            assertEquals(1, keys[2]);
            assertEquals(KeyEvent.KEYCODE_MENU, keys[4]);
            assertEquals(0, keys[5]);
            // A second route for the same release cannot enqueue another Resume.
            assertTrue(game.handleKeyUp(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_MODE)));
            assertArrayEquals(new int[0], KorriSurfaceHost.takeKeys());
        } finally {
            KorriSurfaceHost.detach(attachment);
            surface.release();
        }
    }

    @Test public void gameForwardsHatAndStickMotionButNotTouchAndClearsItOnClose() {
        Game game = Robolectric.buildActivity(Game.class).get();
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
        ReflectionHelpers.setField(game, "surfaceOverlay", new SurfaceView(game));
        MotionEvent.PointerProperties properties = new MotionEvent.PointerProperties();
        properties.id = 0;
        MotionEvent.PointerCoords coords = new MotionEvent.PointerCoords();
        coords.setAxisValue(MotionEvent.AXIS_X, -0.8f);
        coords.setAxisValue(MotionEvent.AXIS_Y, 0.7f);
        coords.setAxisValue(MotionEvent.AXIS_HAT_X, 1);
        coords.setAxisValue(MotionEvent.AXIS_HAT_Y, -1);
        MotionEvent motion = MotionEvent.obtain(0, 0, MotionEvent.ACTION_MOVE, 1,
                new MotionEvent.PointerProperties[] { properties }, new MotionEvent.PointerCoords[] { coords },
                0, 0, 1, 1, 7, 0, android.view.InputDevice.SOURCE_JOYSTICK, 0);
        try {
            assertTrue(game.dispatchGenericMotionEvent(motion));
            assertArrayEquals(new int[] { 7, -800, 700, 1000, -1000 }, KorriSurfaceHost.takeAxes());
            assertTrue(game.handleMotionEvent(null, motion));
            assertArrayEquals(new int[] { 7, -800, 700, 1000, -1000 }, KorriSurfaceHost.takeAxes());
            motion.setAction(MotionEvent.ACTION_CANCEL);
            assertTrue(game.dispatchGenericMotionEvent(motion));
            assertArrayEquals(new int[] { 7, 0, 0, 0, 0 }, KorriSurfaceHost.takeAxes());
            motion.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
            assertTrue(game.dispatchTouchEvent(motion));
            assertTrue(game.dispatchGenericMotionEvent(motion));
            assertArrayEquals(new int[0], KorriSurfaceHost.takeAxes());
            motion.setSource(android.view.InputDevice.SOURCE_DPAD);
            motion.setAction(MotionEvent.ACTION_MOVE);
            assertTrue(game.dispatchGenericMotionEvent(motion));
            KorriSurfaceHost.detach(attachment);
            assertArrayEquals(new int[0], KorriSurfaceHost.takeAxes());
        } finally {
            KorriSurfaceHost.detach(attachment);
            motion.recycle();
            surface.release();
        }
    }

    @Test
    @Config(shadows = com.limelight.shadows.ShadowMoonBridge.class)
    public void rendererFailureRemovesOnlyTheOverlayAndKeepsTheGameAndStreamSurface() {
        Game game = Robolectric.buildActivity(Game.class).get();
        com.limelight.nvstream.NvConnection connection = new com.limelight.nvstream.NvConnection(
                game, new com.limelight.nvstream.http.ComputerDetails.AddressTuple("127.0.0.1", 47989),
                47984, "test", null, null, null, com.limelight.nvstream.NvConnection.AppLaunchPolicy.EXACT);
        // This phone has no NVIDIA accessory service; model the same absence.
        org.robolectric.Shadows.shadowOf(game.getApplication()).declareComponentUnbindable(
                new android.content.ComponentName("com.nvidia.blakepairing", "com.nvidia.blakepairing.AccessoryService"));
        com.limelight.binding.input.ControllerHandler controllers = new com.limelight.binding.input.ControllerHandler(
                game, connection, game, new com.limelight.preferences.PreferenceConfiguration());
        ReflectionHelpers.setField(game, "conn", connection);
        ReflectionHelpers.setField(game, "controllerHandler", controllers);
        // Leave the decoder absent: accidental stream teardown must fail this test.
        android.widget.FrameLayout root = new android.widget.FrameLayout(game);
        SurfaceView stream = new SurfaceView(game);
        SurfaceView overlay = new SurfaceView(game);
        root.addView(stream);
        root.addView(overlay);
        Surface streamSurface = stream.getHolder().getSurface();
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, game::hideSurfaceOverlay);
        ReflectionHelpers.setField(game, "surfaceOverlay", overlay);
        ReflectionHelpers.setField(game, "surfaceOverlayAttachment", attachment);
        connection.setInputHeld(true);
        try {
            KorriSurfaceHost.takeSurface();
            KorriSurfaceHost.acknowledgeRelease(true);
            org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idle();
            assertEquals(0, attachment.released.getCount());
            assertNull(overlay.getParent());
            assertSame(root, stream.getParent());
            assertSame(streamSurface, stream.getHolder().getSurface());
            assertEquals(1, root.getChildCount());
            assertFalse(game.isFinishing());
            assertFalse(ReflectionHelpers.<Boolean>getField(connection, "inputHeld"));
            assertNull(ReflectionHelpers.getField(game, "surfaceOverlay"));
        } finally {
            KorriSurfaceHost.detach(attachment);
            controllers.destroy();
            surface.release();
        }
    }

    @Test public void nativeReleasePrecedesViewRemovalAndGameplayInput() throws Exception {
        String game = new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/Game.java")),
                java.nio.charset.StandardCharsets.UTF_8);
        String close = game.substring(game.indexOf("void hideSurfaceOverlay()"), game.indexOf("public void hideGameMenu()"));
        assertTrue(close.indexOf("KorriSurfaceHost.detach(attachment)") < close.indexOf("removeView(view)"));
        assertTrue(close.indexOf("removeView(view)") < close.indexOf("conn.setInputHeld(false)"));
        assertTrue(close.indexOf("resetStreamInputState()") < close.indexOf("conn.setInputHeld(false)"));
    }

    @Test public void usbHeartbeatAndSensorProducersShareTheConnectionHoldGate() throws Exception {
        String connection = new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/nvstream/NvConnection.java")), java.nio.charset.StandardCharsets.UTF_8);
        String sends = connection.substring(connection.indexOf("public synchronized void sendMouseMove("));
        for (String method : new String[] { "sendMouseMove", "sendControllerInput", "sendKeyboardInput",
                "sendTouchEvent", "sendPenEvent", "sendControllerTouchEvent", "sendControllerMotionEvent", "sendUtf8Text" }) {
            int start = sends.indexOf(method + "(");
            assertTrue(method, start >= 0);
            int next = sends.indexOf("public ", start);
            String body = sends.substring(start, next < 0 ? sends.length() : next);
            assertTrue(method + " must honor the input hold", body.contains("!inputHeld"));
        }
        assertTrue(connection.contains("public synchronized void setInputHeld(boolean held)"));
        assertTrue(connection.contains("KeyboardPacket.KEY_UP"));
        assertTrue(connection.contains("MoonBridge.LI_TOUCH_EVENT_CANCEL_ALL"));
        String controllers = new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/binding/input/ControllerHandler.java")), java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(controllers.contains("conn.sendControllerInput("));
        assertFalse(controllers.contains("MoonBridge.send"));
    }
}
