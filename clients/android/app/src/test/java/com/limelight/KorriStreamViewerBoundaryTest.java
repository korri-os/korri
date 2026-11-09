package com.limelight;

import android.content.Intent;
import android.content.res.Configuration;
import android.preference.PreferenceManager;


import com.limelight.nvstream.http.NvApp;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;

import java.util.Arrays;
import java.util.Collections;

import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriStreamViewerBoundaryTest {
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";

    @Test
    @Config(shadows = com.limelight.shadows.ShadowMoonBridge.class)
    public void viewerDisplayUsesStreamPreferencesAndGamePortraitInversion() {
        android.app.Application app = RuntimeEnvironment.getApplication();
        KorriStreamViewer.initialize(app);
        PreferenceManager.getDefaultSharedPreferences(app).edit()
                .putString("list_resolution", "2176x1812")
                .putBoolean("checkbox_auto_orientation", true)
                .putBoolean("checkbox_auto_invert_video_resolution", true)
                .apply();
        Configuration configuration = new Configuration(app.getResources().getConfiguration());
        configuration.orientation = Configuration.ORIENTATION_LANDSCAPE;
        app.getResources().updateConfiguration(configuration, app.getResources().getDisplayMetrics());
        assertEquals("2176x1812", KorriStreamViewer.viewerDisplay());
        configuration.orientation = Configuration.ORIENTATION_PORTRAIT;
        app.getResources().updateConfiguration(configuration, app.getResources().getDisplayMetrics());
        assertEquals("1812x2176", KorriStreamViewer.viewerDisplay());
        PreferenceManager.getDefaultSharedPreferences(app).edit()
                .putBoolean("checkbox_auto_orientation", false).apply();
        assertEquals("2176x1812", KorriStreamViewer.viewerDisplay());
        PreferenceManager.getDefaultSharedPreferences(app).edit()
                .putBoolean("checkbox_auto_orientation", true)
                .putBoolean("checkbox_auto_invert_video_resolution", false).apply();
        assertEquals("2176x1812", KorriStreamViewer.viewerDisplay());
    }

    @Test
    public void processRecreationCannotStartAnUnownedGameOrCrashItsTeardown() {
        KorriStreamViewer.initialize(RuntimeEnvironment.getApplication());
        Intent stale = new Intent(RuntimeEnvironment.getApplication(), Game.class)
                .putExtra(Game.EXTRA_KORRI_LAUNCH_ID, LAUNCH)
                .putExtra(KorriStreamViewer.EXTRA_GENERATION, 1L);
        ActivityController<Game> activity = Robolectric.buildActivity(Game.class, stale).create();
        assertTrue("a fresh brain has no selected route to resurrect", activity.get().isFinishing());
        activity.destroy();
        assertEquals("Stopped", KorriStreamViewer.status(LAUNCH));
    }

    @Test
    public void effectsNeverBlockTheAndroidMainThread() {
        KorriStreamViewer.initialize(RuntimeEnvironment.getApplication());
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.start(LAUNCH, "peer:47989"));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.background(LAUNCH));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.resume(LAUNCH));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.stop(LAUNCH));
    }

    @Test
    public void invalidOrUnknownLaunchCannotRetargetTheCurrentViewer() {
        KorriStreamViewer.initialize(RuntimeEnvironment.getApplication());
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.status(""));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.status("../../other"));
        assertEquals("Stopped", KorriStreamViewer.status(LAUNCH));
    }

    @Test
    public void appListMustContainExactlyTheLinuxTransportApp() {
        NvApp game = new NvApp("Other Game", "other", 1, false);
        NvApp stream = new NvApp("Korri Stream", "stream", 2, false);
        assertSame(stream, KorriStreamViewer.streamApp(Arrays.asList(game, stream)));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.streamApp(Collections.emptyList()));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.streamApp(Collections.singletonList(game)));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.streamApp(Arrays.asList(stream, stream)));
        assertThrows(IllegalStateException.class, () -> KorriStreamViewer.streamApp(Collections.singletonList(
                new NvApp("Korri Stream", "invalid", 0, false))));
    }
}
