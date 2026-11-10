package com.limelight;

import android.content.Context;
import android.content.SharedPreferences;
import android.preference.PreferenceManager;
import com.limelight.korri.moonlight.KorriMoonlightActionExecutor.Effect;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.StreamContainer;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import java.lang.reflect.Field;
import java.util.Map;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriViewerControlsTest {
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";
    private static final String PREFIX = KorriViewerControls.prefix("0123456789abcdef0123456789abcdef", 1, 1);

    private static void field(Game game, String name, Object value) throws Exception {
        Field field = Game.class.getDeclaredField(name);
        field.setAccessible(true);
        field.set(game, value);
    }

    private Game game(Context context) throws Exception {
        Game game = Robolectric.buildActivity(Game.class).get();
        // Construct the real runtime config without starting Moonlight's native decoder.
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        PreferenceConfiguration config = new PreferenceConfiguration();
        config.flipFaceButtons = prefs.getBoolean("checkbox_flip_face_buttons", false);
        config.sgsrSharpness = prefs.getInt("seekbar_sgsr_sharpness", 20) / 10f;
        config.sgsrEdgeThreshold = prefs.getInt("seekbar_sgsr_edge_threshold", 8) / 255f;
        config.renderMode = 3;
        config.videoScaleMode = PreferenceConfiguration.ScaleMode.FIT;
        config.enableRumble = prefs.getBoolean("checkbox_enable_rumble", true);
        field(game, "displayWidth", 1280);
        field(game, "displayHeight", 720);
        field(game, "prefConfig", config);
        field(game, "korriViewerRequested", true);
        field(game, "streamContainer", new StreamContainer(game, null));
        return game;
    }

    @Test public void realGameApplyAndReadbackAreStreamLocalAndReplayAcrossRecreation() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        prefs.edit().clear().putBoolean("checkbox_flip_face_buttons", false)
                .putInt("seekbar_sgsr_sharpness", 20).apply();
        Game first = game(context);
        Map<String, ?> before = prefs.getAll();
        KorriStreamControlState session = new KorriStreamControlState();
        assertTrue(KorriViewerControls.apply(first, LAUNCH, PREFIX, PREFIX + "set-face-button-flip", "{\"kind\":\"toggle\",\"value\":true}", session));
        assertTrue(first.faceButtonFlip());
        assertTrue(KorriViewerControls.apply(first, LAUNCH, PREFIX, PREFIX + "set-sgsr-sharpness", "{\"kind\":\"range\",\"value\":31}", session));
        assertEquals(31, first.sgsrSharpness());
        assertEquals(before, prefs.getAll());
        first.applyLivePrefs();
        assertTrue(first.faceButtonFlip());
        assertEquals(31, first.sgsrSharpness());
        Game recreated = game(context);
        KorriViewerControls.restore(recreated, session);
        assertTrue(recreated.faceButtonFlip());
        assertEquals(31, recreated.sgsrSharpness());
        assertEquals(before, prefs.getAll());
        Game nextStream = game(context);
        KorriViewerControls.restore(nextStream, new KorriStreamControlState());
        assertFalse(nextStream.faceButtonFlip());
        assertEquals(20, nextStream.sgsrSharpness());
        JSONObject snapshot = new JSONObject(KorriViewerControls.snapshot(recreated, PREFIX));
        boolean found = false;
        for (int i = 0; i < snapshot.getJSONArray("controls").length(); i++) {
            JSONObject control = snapshot.getJSONArray("controls").getJSONObject(i);
            if (control.getString("id").endsWith("set-face-button-flip")) {
                found = true;
                assertTrue(control.getJSONObject("interaction").getBoolean("value"));
            }
        }
        assertTrue(found);
    }

    @Test public void zoomGeometryIsRetainedOnlyByTheSameStreamAndNeverWrittenToPreferences() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        Game game = game(context);
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        Map<String, ?> before = prefs.getAll();
        android.widget.FrameLayout parent = new android.widget.FrameLayout(game);
        android.view.View firstView = new android.view.View(game);
        parent.addView(firstView);
        com.limelight.utils.PanZoomHandler first = new com.limelight.utils.PanZoomHandler(context, game, firstView, parent, new PreferenceConfiguration());
        first.setInitialZoomAndPan(2.0f, -10.5f, -20.5f);
        KorriStreamControlState session = new KorriStreamControlState();
        session.captureZoomPan(first);
        android.view.View nextView = new android.view.View(game);
        parent.addView(nextView);
        com.limelight.utils.PanZoomHandler recreated = new com.limelight.utils.PanZoomHandler(context, game, nextView, parent, new PreferenceConfiguration());
        session.restoreZoomPan(recreated);
        assertEquals(2.0f, recreated.getScaleFactor(), 0);
        assertEquals(-10.5f, recreated.getChildX(), 0);
        assertEquals(-20.5f, recreated.getChildY(), 0);
        android.view.View freshView = new android.view.View(game);
        parent.addView(freshView);
        com.limelight.utils.PanZoomHandler fresh = new com.limelight.utils.PanZoomHandler(context, game, freshView, parent, new PreferenceConfiguration());
        new KorriStreamControlState().restoreZoomPan(fresh);
        assertEquals(1.0f, fresh.getScaleFactor(), 0);
        assertEquals(before, prefs.getAll());
    }

    @Test public void fillOffRetainsTheActualFitResultAcrossAStretchBaselineRecreation() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        Game first = game(context);
        first.viewerRestoreScaleMode(PreferenceConfiguration.ScaleMode.STRETCH);
        KorriStreamControlState session = new KorriStreamControlState();
        assertTrue(KorriViewerControls.apply(first, LAUNCH, PREFIX, PREFIX + "set-fill-mode", "{\"kind\":\"toggle\",\"value\":true}", session));
        assertTrue(KorriViewerControls.apply(first, LAUNCH, PREFIX, PREFIX + "set-fill-mode", "{\"kind\":\"toggle\",\"value\":false}", session));
        assertEquals(PreferenceConfiguration.ScaleMode.FIT, first.viewerScaleMode());
        Game recreated = game(context);
        recreated.viewerRestoreScaleMode(PreferenceConfiguration.ScaleMode.STRETCH);
        KorriViewerControls.restore(recreated, session);
        assertEquals(PreferenceConfiguration.ScaleMode.FIT, recreated.viewerScaleMode());
    }

    @Test public void pictureInPictureUpdatesAutoEntryAndRumbleDisablesBothMotorKindsWithoutSaving() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        Game game = game(context);
        org.robolectric.Shadows.shadowOf(game.getPackageManager()).setSystemFeature(android.content.pm.PackageManager.FEATURE_PICTURE_IN_PICTURE, true);
        field(game, "connected", true);
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        Map<String, ?> before = prefs.getAll();
        KorriStreamControlState session = new KorriStreamControlState();
        assertTrue(KorriViewerControls.apply(game, LAUNCH, PREFIX, PREFIX + "set-picture-in-picture", "{\"kind\":\"toggle\",\"value\":true}", session));
        assertTrue(game.pictureInPicture());
        Field auto = Game.class.getDeclaredField("autoEnterPip");
        auto.setAccessible(true);
        assertTrue(auto.getBoolean(game));
        game.setRumble(false);
        assertFalse(game.rumble());
        // With rumble disabled no absent controller endpoint is touched, including triggers.
        game.rumble((short) 0, (short) 1, (short) 1);
        game.rumbleTriggers((short) 0, (short) 1, (short) 1);
        assertEquals(before, prefs.getAll());
    }

    @Test public void staleIdentityAndInvalidRangesHaveNoEffect() throws Exception {
        Context context = androidx.test.core.app.ApplicationProvider.getApplicationContext();
        Game game = game(context);
        KorriStreamControlState state = new KorriStreamControlState();
        int before = game.sgsrSharpness();
        assertFalse(KorriViewerControls.apply(game, LAUNCH, PREFIX, "viewer:other:set-sgsr-sharpness", "{\"kind\":\"range\",\"value\":10}", state));
        for (String wrong : new String[] { KorriViewerControls.prefix(LAUNCH, 2, 1), KorriViewerControls.prefix(LAUNCH, 1, 2) }) {
            assertFalse(KorriViewerControls.apply(game, LAUNCH, PREFIX, wrong + "set-sgsr-sharpness", "{\"kind\":\"range\",\"value\":10}", state));
        }
        for (String raw : new String[] {"51", "-1", "1.5", "true", "\"12\""}) {
            assertFalse(KorriViewerControls.apply(game, LAUNCH, PREFIX, PREFIX + "set-sgsr-sharpness", "{\"kind\":\"range\",\"value\":" + raw + "}", state));
        }
        assertEquals(before, game.sgsrSharpness());
    }

    @Test public void noTouchViewerNeverDeclaresTouchOnlyControlsOrDuplicateOverlays() {
        for (Effect effect : new Effect[] {Effect.TOGGLE_KEYBOARD, Effect.TOGGLE_FULL_KEYBOARD, Effect.SET_ZOOM_MODE,
                Effect.SET_MOUSE_MODE, Effect.TOGGLE_KEYBOARD_CONTROLLER, Effect.SWITCH_TOUCH_SENSITIVITY}) {
            assertFalse(KorriViewerControls.included(effect, false));
            assertTrue(KorriViewerControls.included(effect, true));
        }
        assertFalse(KorriViewerControls.included(Effect.TOGGLE_HUD, true));
        assertFalse(KorriViewerControls.included(Effect.TOGGLE_FLOATING_MENU, true));
        assertTrue(KorriViewerControls.included(Effect.SET_FILL_MODE, false));
    }
}
