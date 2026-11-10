package com.limelight;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.os.Binder;
import android.view.WindowManager;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.shadows.MediaCodecInfoBuilder;
import org.robolectric.shadows.ShadowMediaCodecList;
import org.robolectric.util.ReflectionHelpers;

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

    /**
     * The real Game, created as a Korri viewer for one selected launch and
     * shown on a 1972x1248 window (the folded Fold in landscape). For every
     * scale mode and for a stream that is not the screen's native size, the
     * window draws through the cutout. Under a 104 px left cutout the stream
     * keeps the whole window and its scale mode's shape, and the gameplay
     * overlay that Game opens lays out beside the cutout.
     */
    @Config(sdk = 33, shadows = {
            com.limelight.shadows.ShadowMoonBridge.class,
            com.limelight.shadows.ShadowGameManager.class})
    @Test public void game_draws_through_the_cutout_and_keeps_its_overlay_out_of_it() throws Exception {
        String[] modes = {"fit", "fill", "stretch"};
        int[][] expected = {{1972, 1109}, {2218, 1248}, {1972, 1248}};
        for (int i = 0; i < modes.length; i++) {
            org.robolectric.android.controller.ActivityController<Game> controller = startViewer(modes[i]);
            try {
                Game game = controller.get();
                assertEquals(modes[i], WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS,
                        game.getWindow().getAttributes().layoutInDisplayCutoutMode);

                game.showGameMenu();
                android.view.ViewGroup content = game.findViewById(android.R.id.content);
                android.view.View overlay = content.getChildAt(content.getChildCount() - 1);
                assertTrue(modes[i], overlay instanceof android.view.SurfaceView);
                android.view.View decor = game.getWindow().getDecorView();
                androidx.core.view.ViewCompat.dispatchApplyWindowInsets(decor,
                        new androidx.core.view.WindowInsetsCompat.Builder()
                                .setInsets(androidx.core.view.WindowInsetsCompat.Type.displayCutout(),
                                        androidx.core.graphics.Insets.of(104, 0, 0, 0))
                                .build());
                decor.measure(android.view.View.MeasureSpec.makeMeasureSpec(1972, android.view.View.MeasureSpec.EXACTLY),
                        android.view.View.MeasureSpec.makeMeasureSpec(1248, android.view.View.MeasureSpec.EXACTLY));
                decor.layout(0, 0, 1972, 1248);

                assertArrayEquals(modes[i], new int[] {0, 0, 1972, 1248}, boundsInWindow(content));
                android.view.View stream = game.findViewById(R.id.streamContainer);
                assertArrayEquals(modes[i], expected[i], new int[] {stream.getWidth(), stream.getHeight()});
                assertArrayEquals(modes[i], new int[] {104, 0, 1868, 1248}, boundsInWindow(overlay));
            } finally {
                ReflectionHelpers.setField(ReflectionHelpers.getStaticField(KorriStreamViewer.class, "instance"),
                        "session", null);
            }
        }
    }

    private static int[] boundsInWindow(android.view.View view) {
        int[] location = new int[2];
        view.getLocationInWindow(location);
        return new int[] {location[0], location[1], view.getWidth(), view.getHeight()};
    }

    /**
     * Game as KorriStreamViewer opens it for a selected far launch: a
     * 1920x1080 stream, which is not this screen's native size.
     */
    private static org.robolectric.android.controller.ActivityController<Game> startViewer(String scaleMode)
            throws Exception {
        android.app.Application app = RuntimeEnvironment.getApplication();
        // Robolectric has no hardware decoder; Game refuses to stream without AVC.
        ShadowMediaCodecList.addCodec(MediaCodecInfoBuilder.newBuilder()
                .setName("OMX.qcom.video.decoder.avc").setIsEncoder(false)
                .setCapabilities(MediaCodecInfoBuilder.CodecCapabilitiesBuilder.newBuilder()
                        .setMediaFormat(MediaFormat.createVideoFormat("video/avc", 1920, 1080))
                        .setColorFormats(new int[] {MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible})
                        .build())
                .build());
        // The controller-extension service Game binds needs a binder to answer.
        Shadows.shadowOf(app).setComponentNameAndServiceForBindService(
                new ComponentName("test", "test"), new Binder());
        // The launch korrid selected: KorriStreamViewer keeps it as its session.
        KorriStreamViewer.initialize(app);
        Object viewer = ReflectionHelpers.getStaticField(KorriStreamViewer.class, "instance");
        Object session = ReflectionHelpers.callConstructor(
                Class.forName("com.limelight.KorriStreamViewer$Session"),
                ReflectionHelpers.ClassParameter.from(String.class, "388388388388388388388388388388aa"));
        ReflectionHelpers.setField(viewer, "session", session);
        KorriStreamViewerState state = ReflectionHelpers.getField(session, "state");
        Intent intent = new Intent(app, Game.class)
                .putExtra(Game.EXTRA_HOST, "127.0.0.1")
                .putExtra(Game.EXTRA_PORT, 47989)
                .putExtra(Game.EXTRA_APP_ID, 1)
                .putExtra(Game.EXTRA_UNIQUEID, "0123456789ABCDEF")
                .putExtra(Game.EXTRA_WIDTH, 1920)
                .putExtra(Game.EXTRA_HEIGHT, 1080)
                .putExtra(Game.EXTRA_VIDEO_SCALE_MODE, scaleMode)
                .putExtra(Game.EXTRA_KORRI_LAUNCH_ID, state.launchId)
                .putExtra(KorriStreamViewer.EXTRA_GENERATION, state.generation);
        return Robolectric.buildActivity(Game.class, intent).create().visible();
    }
}
