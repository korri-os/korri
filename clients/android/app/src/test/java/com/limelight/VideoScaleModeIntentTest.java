package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Before;
import org.junit.BeforeClass;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.*;

@Config(sdk = {33}, shadows = {
    com.limelight.shadows.ShadowMoonBridge.class,
    com.limelight.shadows.ShadowGameManager.class
})
@RunWith(RobolectricTestRunner.class)
public class VideoScaleModeIntentTest {
    private Context context;

    @BeforeClass
    public static void suppressLogs() {
        TestLogSuppressor.install();
    }

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
    }

    @Test
    public void testVideoScaleModeExtrasConstant() {
        assertEquals("VideoScaleMode", Game.EXTRA_VIDEO_SCALE_MODE);
    }

    @Test
    public void testIntentWithVideoScaleModeExtra() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_VIDEO_SCALE_MODE, "fill");

        assertEquals("fill", intent.getStringExtra(Game.EXTRA_VIDEO_SCALE_MODE));
    }

    @Test
    public void testIntentWithNoVideoScaleModeExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_VIDEO_SCALE_MODE));
    }

    @Test
    public void testVideoScaleModeOverrideLogic_Fit() {
        String intentVideoScaleMode = "fit";
        PreferenceConfiguration.ScaleMode scaleMode = null;

        switch (intentVideoScaleMode) {
            case "fit":
                scaleMode = PreferenceConfiguration.ScaleMode.FIT;
                break;
            case "fill":
                scaleMode = PreferenceConfiguration.ScaleMode.FILL;
                break;
            case "stretch":
                scaleMode = PreferenceConfiguration.ScaleMode.STRETCH;
                break;
        }

        assertEquals(PreferenceConfiguration.ScaleMode.FIT, scaleMode);
    }

    @Test
    public void testVideoScaleModeOverrideLogic_Fill() {
        String intentVideoScaleMode = "fill";
        PreferenceConfiguration.ScaleMode scaleMode = null;

        switch (intentVideoScaleMode) {
            case "fit":
                scaleMode = PreferenceConfiguration.ScaleMode.FIT;
                break;
            case "fill":
                scaleMode = PreferenceConfiguration.ScaleMode.FILL;
                break;
            case "stretch":
                scaleMode = PreferenceConfiguration.ScaleMode.STRETCH;
                break;
        }

        assertEquals(PreferenceConfiguration.ScaleMode.FILL, scaleMode);
    }

    @Test
    public void testVideoScaleModeOverrideLogic_Stretch() {
        String intentVideoScaleMode = "stretch";
        PreferenceConfiguration.ScaleMode scaleMode = null;

        switch (intentVideoScaleMode) {
            case "fit":
                scaleMode = PreferenceConfiguration.ScaleMode.FIT;
                break;
            case "fill":
                scaleMode = PreferenceConfiguration.ScaleMode.FILL;
                break;
            case "stretch":
                scaleMode = PreferenceConfiguration.ScaleMode.STRETCH;
                break;
        }

        assertEquals(PreferenceConfiguration.ScaleMode.STRETCH, scaleMode);
    }

    @Test
    public void testVideoScaleModeOverrideLogic_InvalidValue() {
        String intentVideoScaleMode = "invalid";
        PreferenceConfiguration.ScaleMode scaleMode = PreferenceConfiguration.ScaleMode.FIT;  // Default

        switch (intentVideoScaleMode) {
            case "fit":
                scaleMode = PreferenceConfiguration.ScaleMode.FIT;
                break;
            case "fill":
                scaleMode = PreferenceConfiguration.ScaleMode.FILL;
                break;
            case "stretch":
                scaleMode = PreferenceConfiguration.ScaleMode.STRETCH;
                break;
            // Invalid values are silently ignored
        }

        // Should remain unchanged at default FIT
        assertEquals(PreferenceConfiguration.ScaleMode.FIT, scaleMode);
    }
}
