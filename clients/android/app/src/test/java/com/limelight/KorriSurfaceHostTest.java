package com.limelight;

import android.graphics.SurfaceTexture;
import android.view.KeyEvent;
import android.view.Surface;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriSurfaceHostTest {
    @Test public void rustMustReleaseBeforeSurfaceDestructionReturns() throws Exception {
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 2, () -> fail("Unexpected renderer failure"));
        assertSame(surface, KorriSurfaceHost.takeSurface());
        assertNull(KorriSurfaceHost.takeSurface());
        CountDownLatch detached = new CountDownLatch(1);
        Thread destroy = new Thread(() -> { KorriSurfaceHost.detach(attachment); detached.countDown(); });
        destroy.start();
        try {
            assertFalse("Android must not destroy a Surface still held by EGL", detached.await(50, TimeUnit.MILLISECONDS));
            assertTrue(KorriSurfaceHost.closing());
        } finally {
            KorriSurfaceHost.acknowledgeRelease(false);
            destroy.join(1000);
        }
        assertEquals(0, detached.getCount());
        surface.release();
    }

    @Test public void resumeAndRepeatedOpenNeverRetainKeysOrPendingSurfaces() {
        for (int cycle = 0; cycle < 5; cycle++) {
            Surface surface = new Surface(new SurfaceTexture(0));
            KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 2, () -> fail("Unexpected renderer failure"));
            assertEquals(2, KorriSurfaceHost.density(), 0);
            assertArrayEquals(new int[0], KorriSurfaceHost.takeKeys());
            KeyEvent down = new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_DOWN);
            assertTrue(KorriSurfaceHost.forward(down));
            int[] keys = KorriSurfaceHost.takeKeys();
            assertEquals(3, keys.length);
            assertEquals(KeyEvent.KEYCODE_DPAD_DOWN, keys[1]);
            assertEquals(1, keys[2]);
            assertTrue(KorriSurfaceHost.forward(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_DPAD_DOWN)));
            assertEquals(0, KorriSurfaceHost.takeKeys()[2]);
            KorriSurfaceHost.resized(attachment);
            assertTrue(KorriSurfaceHost.takeResized());
            assertFalse(KorriSurfaceHost.takeResized());
            // Closing before Rust takes the Surface requires no native ack.
            KorriSurfaceHost.detach(attachment);
            assertFalse(KorriSurfaceHost.forward(down));
            assertNull(KorriSurfaceHost.takeSurface());
            surface.release();
        }
    }

    @Test public void rendererFailureReleasesBeforeTheHostReceivesItsFailureCallback() {
        Surface surface = new Surface(new SurfaceTexture(0));
        java.util.concurrent.atomic.AtomicInteger failures = new java.util.concurrent.atomic.AtomicInteger();
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, failures::incrementAndGet);
        KorriSurfaceHost.takeSurface();
        KorriSurfaceHost.acknowledgeRelease(true);
        org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper()).idle();
        assertEquals(0, attachment.released.getCount());
        assertEquals(1, failures.get());
        assertNull(KorriSurfaceHost.takeSurface());
        KorriSurfaceHost.detach(attachment);
        surface.release();
    }

    @Test public void everyRustSurfaceBridgeMethodHasAnExactKeepRule() throws Exception {
        String rules = new String(java.nio.file.Files.readAllBytes(java.nio.file.Path.of("proguard-rules.pro")),
                java.nio.charset.StandardCharsets.UTF_8);
        for (String method : new String[] { "android.view.Surface shellTakeOverlaySurface()",
                "float shellOverlayDensity()", "boolean shellOverlayResized()", "boolean shellOverlayClosing()",
                "void shellOverlayReleased(boolean)", "int[] shellTakeOverlayKeys()" }) {
            assertTrue(method, rules.contains("public " + method + ";"));
        }
    }

    @Test public void guideBecomesBackOnlyWhileTheSurfaceOwnsInput() {
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
        KorriSurfaceHost.forward(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_MODE));
        assertEquals(KeyEvent.KEYCODE_BACK, KorriSurfaceHost.takeKeys()[1]);
        KorriSurfaceHost.detach(attachment);
        surface.release();
    }
}
