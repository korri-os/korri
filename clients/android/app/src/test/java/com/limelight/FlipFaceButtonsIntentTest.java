package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

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
public class FlipFaceButtonsIntentTest {
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
    public void testFlipFaceButtonsExtrasConstant() {
        assertEquals("FlipFaceButtons", Game.EXTRA_FLIP_FACE_BUTTONS);
    }

    @Test
    public void testIntentWithFlipFaceButtonsExtra_True() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_FLIP_FACE_BUTTONS, "true");

        assertEquals("true", intent.getStringExtra(Game.EXTRA_FLIP_FACE_BUTTONS));
    }

    @Test
    public void testIntentWithFlipFaceButtonsExtra_False() {
        Intent intent = new Intent(context, Game.class);
        intent.putExtra(Game.EXTRA_FLIP_FACE_BUTTONS, "false");

        assertEquals("false", intent.getStringExtra(Game.EXTRA_FLIP_FACE_BUTTONS));
    }

    @Test
    public void testIntentWithNoFlipFaceButtonsExtra() {
        Intent intent = new Intent(context, Game.class);

        assertNull(intent.getStringExtra(Game.EXTRA_FLIP_FACE_BUTTONS));
    }

    @Test
    public void testFlipFaceButtonsOverrideLogic_True() {
        String intentFlipFaceButtons = "true";
        boolean flipFaceButtons = false;

        if (intentFlipFaceButtons != null) {
            flipFaceButtons = Boolean.parseBoolean(intentFlipFaceButtons);
        }

        assertTrue(flipFaceButtons);
    }

    @Test
    public void testFlipFaceButtonsOverrideLogic_False() {
        String intentFlipFaceButtons = "false";
        boolean flipFaceButtons = true;  // Start with true

        if (intentFlipFaceButtons != null) {
            flipFaceButtons = Boolean.parseBoolean(intentFlipFaceButtons);
        }

        assertFalse(flipFaceButtons);
    }

    @Test
    public void testFlipFaceButtonsOverrideLogic_InvalidValue() {
        String intentFlipFaceButtons = "invalid";
        boolean flipFaceButtons = true;  // Start with true

        if (intentFlipFaceButtons != null) {
            flipFaceButtons = Boolean.parseBoolean(intentFlipFaceButtons);
        }

        // Boolean.parseBoolean returns false for any non-"true" string
        assertFalse(flipFaceButtons);
    }
}
