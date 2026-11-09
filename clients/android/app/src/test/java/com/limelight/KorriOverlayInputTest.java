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
