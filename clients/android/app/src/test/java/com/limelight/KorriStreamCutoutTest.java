package com.limelight;

import android.app.Activity;
import android.view.WindowManager;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.nio.file.Files;
import java.nio.file.Paths;

import static org.junit.Assert.*;

/** #388: the stream window always draws through the camera cutout. */
@RunWith(RobolectricTestRunner.class)
public class KorriStreamCutoutTest {
    private static WindowManager.LayoutParams applied() {
        Activity activity = Robolectric.buildActivity(Activity.class).create().get();
        WindowManager.LayoutParams before = new WindowManager.LayoutParams();
        before.copyFrom(activity.getWindow().getAttributes());
        KorriStreamCutout.drawThroughCutout(activity.getWindow());
        WindowManager.LayoutParams after = activity.getWindow().getAttributes();
        // Only the cutout mode changes: size, gravity and flags stay as they were.
        assertEquals(before.width, after.width);
        assertEquals(before.height, after.height);
        assertEquals(before.gravity, after.gravity);
        assertEquals(before.flags, after.flags);
        return after;
    }

    @Config(sdk = 33)
    @Test public void api_30_and_later_use_always() {
        assertEquals(WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS,
                applied().layoutInDisplayCutoutMode);
    }

    @Config(sdk = 28)
    @Test public void api_28_and_29_use_short_edges() {
        assertEquals(WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES,
                applied().layoutInDisplayCutoutMode);
    }

    /**
     * Layout check at the Fold's measured sizes. Folded: 1248x1972 with a
     * 104 px cutout inset on the top (portrait) or left (landscape), from the
     * #382 spike's logcat. Unfolded: 2448x1848, the screen mode the #398 brief
     * records. With the cutout drawn through, the stream window is the whole
     * screen; the scale mode alone decides the picture inside it.
     */
    @Config(sdk = 33)
    @Test public void the_scale_mode_alone_decides_the_picture_at_fold_sizes() {
        int[][] screens = {{1248, 1972}, {1972, 1248}, {1848, 2448}, {2448, 1848}};
        int[][] streams = {{1920, 1080}, {1080, 1920}, {2448, 1848}, {1972, 1248}};
        for (int[] screen : screens) {
            for (int[] stream : streams) {
                double aspect = (double) stream[0] / stream[1];
                int[] fit = measure(screen, aspect, false);
                assertTrue(fit[0] <= screen[0] && fit[1] <= screen[1]);
                assertTrue(fit[0] == screen[0] || fit[1] == screen[1]);
                assertEquals(aspect, (double) fit[0] / fit[1], 0.01);
                int[] fill = measure(screen, aspect, true);
                assertTrue(fill[0] >= screen[0] && fill[1] >= screen[1]);
                assertEquals(aspect, (double) fill[0] / fill[1], 0.01);
                assertArrayEquals(screen, measure(screen, 0, false));
            }
        }
    }

    private static int[] measure(int[] screen, double aspect, boolean fill) {
        com.limelight.ui.StreamContainer container = new com.limelight.ui.StreamContainer(
                androidx.test.core.app.ApplicationProvider.getApplicationContext(), null);
        container.setDesiredAspectRatio(aspect);
        container.setFillDisplay(fill);
        container.measure(
                android.view.View.MeasureSpec.makeMeasureSpec(screen[0], android.view.View.MeasureSpec.EXACTLY),
                android.view.View.MeasureSpec.makeMeasureSpec(screen[1], android.view.View.MeasureSpec.EXACTLY));
        return new int[] {container.getMeasuredWidth(), container.getMeasuredHeight()};
    }

    /**
     * The overlay lays out inside the cutout's safe area and follows it when
     * the phone rotates or folds: 104 px on the left in folded landscape, on
     * the top in folded portrait, none where no cutout is reported.
     */
    @Config(sdk = 33)
    @Test public void the_overlay_keeps_out_of_the_cutout_and_follows_it() {
        Activity activity = Robolectric.buildActivity(Activity.class).create().start().resume().get();
        android.widget.FrameLayout root = new android.widget.FrameLayout(activity);
        activity.setContentView(root);
        android.view.View overlay = new android.view.View(activity);
        root.addView(overlay, new android.widget.FrameLayout.LayoutParams(
                android.widget.FrameLayout.LayoutParams.MATCH_PARENT,
                android.widget.FrameLayout.LayoutParams.MATCH_PARENT));
        KorriStreamCutout.keepOutOfCutout(overlay);

        dispatchCutout(overlay, androidx.core.graphics.Insets.of(104, 0, 0, 0));
        assertMargins(overlay, 104, 0, 0, 0);
        dispatchCutout(overlay, androidx.core.graphics.Insets.of(0, 104, 0, 0));
        assertMargins(overlay, 0, 104, 0, 0);
        dispatchCutout(overlay, androidx.core.graphics.Insets.NONE);
        assertMargins(overlay, 0, 0, 0, 0);
    }

    private static void dispatchCutout(android.view.View view, androidx.core.graphics.Insets cutout) {
        androidx.core.view.WindowInsetsCompat insets = new androidx.core.view.WindowInsetsCompat.Builder()
                .setInsets(androidx.core.view.WindowInsetsCompat.Type.displayCutout(), cutout)
                .build();
        androidx.core.view.ViewCompat.dispatchApplyWindowInsets(view, insets);
    }

    private static void assertMargins(android.view.View view, int left, int top, int right, int bottom) {
        android.view.ViewGroup.MarginLayoutParams params =
                (android.view.ViewGroup.MarginLayoutParams) view.getLayoutParams();
        assertArrayEquals(new int[] {left, top, right, bottom},
                new int[] {params.leftMargin, params.topMargin, params.rightMargin, params.bottomMargin});
    }

    /** Game cannot start under Robolectric (native decoder), so guard its source. */
    @Test public void game_applies_it_for_every_scale_mode_and_resolution() throws Exception {
        String game = new String(Files.readAllBytes(
                Paths.get("src/main/java/com/limelight/Game.java")));
        assertEquals(1, game.split("KorriStreamCutout.drawThroughCutout\\(getWindow\\(\\)\\);", -1).length - 1);
        assertFalse(game.contains("shouldIgnoreInsetsForResolution"));
        assertFalse(game.contains("layoutInDisplayCutoutMode"));
        // The hosted gameplay overlay is kept out of the cutout.
        String overlay = game.substring(game.indexOf("private void showSurfaceOverlay()"));
        overlay = overlay.substring(0, overlay.indexOf("\n    }\n"));
        assertTrue(overlay.contains("KorriStreamCutout.keepOutOfCutout(view);"));
        // The call sits at onCreate's top level, not under a scale-mode or resolution branch.
        String before = game.substring(0, game.indexOf("KorriStreamCutout.drawThroughCutout("));
        String line = before.substring(before.lastIndexOf('\n') + 1);
        assertEquals("        ", line);
        assertTrue(before.lastIndexOf("setPreferredOrientationForActivity();")
                > before.lastIndexOf("{"));
    }
}
