package com.limelight;

import android.content.Context;
import android.content.Intent;
import android.graphics.SurfaceTexture;
import android.view.KeyEvent;
import android.view.Surface;
import android.view.SurfaceView;
import com.simonwjackson.korri.korrid.KorriAppLauncher;
import com.simonwjackson.korri.korrid.KorriGuideService;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Map;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriViewerGuideTest {
    private static Field field(Class<?> type, String name) throws Exception {
        Field field = type.getDeclaredField(name);
        field.setAccessible(true);
        return field;
    }

    @Test public void viewerJniAndNativeAppKeepRulesAndSignaturesCoexist() throws Exception {
        String rules = new String(java.nio.file.Files.readAllBytes(java.nio.file.Path.of("proguard-rules.pro")), java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(rules.contains("public static java.lang.String viewerSnapshot(java.lang.String);"));
        assertTrue(rules.contains("public static java.lang.String viewerApply(java.lang.String, java.lang.String, java.lang.String);"));
        assertTrue(rules.contains("-keep class com.simonwjackson.korri.korrid.KorriAppLauncher"));
        assertTrue(rules.contains("-keep class dev.korri.savebridge.** { *; }"));
        // Inspect the real Java descriptors without loading korrid's ARM library on the JVM.
        Class<?> server = Class.forName("com.simonwjackson.korri.korrid.KorridServer", false, getClass().getClassLoader());
        assertEquals(String.class, server.getDeclaredMethod("viewerSnapshot", String.class).getReturnType());
        assertEquals(String.class, server.getDeclaredMethod("viewerApply", String.class, String.class, String.class).getReturnType());
        assertEquals(int.class, server.getDeclaredMethod("reportInstalledApps", String.class).getReturnType());
        assertEquals(void.class, server.getDeclaredMethod("appLeft", String.class, boolean.class).getReturnType());
    }

    @Test public void globalGuideRoutesFocusedStreamToItsSurfaceButExternalAppsToKorri() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        KorriAppLauncher.initialize(context);
        KorriStreamViewer.initialize(context);
        Object host = field(KorriStreamViewer.class, "instance").get(null);
        Class<?> sessionType = Class.forName("com.limelight.KorriStreamViewer$Session");
        Constructor<?> constructor = sessionType.getDeclaredConstructor(String.class);
        constructor.setAccessible(true);
        String launch = "0123456789abcdef0123456789abcdef";
        Object session = constructor.newInstance(launch);
        field(KorriStreamViewer.class, "session").set(host, session);
        KorriStreamViewerState state = (KorriStreamViewerState) field(sessionType, "state").get(session);
        long lease = state.attach(launch, state.generation);
        state.opening(lease);
        state.connected(lease);
        state.focus(lease, true);
        Game game = Robolectric.buildActivity(Game.class).get();
        field(Game.class, "korriViewerRequested").set(game, true);
        field(Game.class, "surfaceOverlay").set(game, new SurfaceView(game));
        @SuppressWarnings("unchecked")
        Map<Long, Game> activities = (Map<Long, Game>) field(sessionType, "activities").get(session);
        activities.put(lease, game);
        Surface surface = new Surface(new SurfaceTexture(0));
        KorriSurfaceHost.Attachment attachment = KorriSurfaceHost.attach(surface, 1, () -> fail("Unexpected renderer failure"));
        KorriGuideService service = Robolectric.buildService(KorriGuideService.class).get();
        Method key = KorriGuideService.class.getDeclaredMethod("onKeyEvent", KeyEvent.class);
        key.setAccessible(true);
        try {
            assertTrue((Boolean) key.invoke(service, new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_MODE)));
            assertTrue((Boolean) key.invoke(service, new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_MODE)));
            int[] keys = KorriSurfaceHost.takeKeys();
            assertEquals(6, keys.length);
            assertEquals(KeyEvent.KEYCODE_MENU, keys[1]);
            assertEquals(1, keys[2]);
            assertEquals(KeyEvent.KEYCODE_MENU, keys[4]);
            assertEquals(0, keys[5]);
            assertNull(Shadows.shadowOf((android.app.Application) context).getNextStartedActivity());
            state.focus(lease, false);
            assertTrue((Boolean) key.invoke(service, new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_MODE)));
            Intent opened = Shadows.shadowOf((android.app.Application) context).getNextStartedActivity();
            assertNotNull(opened);
            assertEquals(KorriShellActivity.class.getName(), opened.getComponent().getClassName());
            assertFalse((Boolean) key.invoke(service, new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_A)));
            assertEquals(0, KorriSurfaceHost.takeKeys().length);
        } finally {
            KorriSurfaceHost.detach(attachment);
            surface.release();
            field(KorriStreamViewer.class, "instance").set(null, null);
        }
    }
}
