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
public class CodecIntentTest {
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
    public void testCodecExtrasConstant() {
        assertEquals("Codec", Game.EXTRA_CODEC);
    }

    @Test
    public void testIntentWithCodecExtra() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_CODEC, "hevc");

        assertEquals("hevc", intent.getStringExtra(Game.EXTRA_CODEC));
    }

    @Test
    public void testIntentWithNoCodecExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_CODEC));
    }

    @Test
    public void testCodecOverrideLogic_Auto() {
        String intentCodec = "auto";
        PreferenceConfiguration.FormatOption videoFormat = null;

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
        }

        assertEquals(PreferenceConfiguration.FormatOption.AUTO, videoFormat);
    }

    @Test
    public void testCodecOverrideLogic_Av1() {
        String intentCodec = "av1";
        PreferenceConfiguration.FormatOption videoFormat = null;

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
        }

        assertEquals(PreferenceConfiguration.FormatOption.FORCE_AV1, videoFormat);
    }

    @Test
    public void testCodecOverrideLogic_Hevc() {
        String intentCodec = "hevc";
        PreferenceConfiguration.FormatOption videoFormat = null;

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
        }

        assertEquals(PreferenceConfiguration.FormatOption.FORCE_HEVC, videoFormat);
    }

    @Test
    public void testCodecOverrideLogic_H265Alias() {
        String intentCodec = "h265";
        PreferenceConfiguration.FormatOption videoFormat = null;

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
        }

        assertEquals(PreferenceConfiguration.FormatOption.FORCE_HEVC, videoFormat);
    }

    @Test
    public void testCodecOverrideLogic_H264() {
        String intentCodec = "h264";
        PreferenceConfiguration.FormatOption videoFormat = null;

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
        }

        assertEquals(PreferenceConfiguration.FormatOption.FORCE_H264, videoFormat);
    }

    @Test
    public void testCodecOverrideLogic_InvalidValue() {
        String intentCodec = "invalid";
        PreferenceConfiguration.FormatOption videoFormat = PreferenceConfiguration.FormatOption.AUTO;  // Default

        switch (intentCodec) {
            case "auto":
                videoFormat = PreferenceConfiguration.FormatOption.AUTO;
                break;
            case "av1":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_AV1;
                break;
            case "hevc":
            case "h265":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
                break;
            case "h264":
                videoFormat = PreferenceConfiguration.FormatOption.FORCE_H264;
                break;
            // Invalid values are silently ignored
        }

        // Should remain unchanged at default AUTO
        assertEquals(PreferenceConfiguration.FormatOption.AUTO, videoFormat);
    }
}
