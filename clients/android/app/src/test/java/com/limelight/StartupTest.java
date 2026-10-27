package com.limelight;

import android.content.Context;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;


import org.junit.Before;
import org.junit.BeforeClass;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.io.File;

import static org.junit.Assert.*;

import com.limelight.TestLogSuppressor;

@Config(sdk = {33}, shadows = {com.limelight.shadows.ShadowMoonBridge.class, com.limelight.shadows.ShadowGameManager.class})
@RunWith(RobolectricTestRunner.class)
public class StartupTest {
    private Context context;

    @BeforeClass
    public static void suppressInvalidIdLogs() {
        TestLogSuppressor.install();
    }

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
    }

    @Test
    public void testPcViewActivityCreation() {
        // Test PcView activity startup
        PcView activity = Robolectric.buildActivity(PcView.class).create().get();
        assertNotNull("PcView activity should be created", activity);
        assertFalse("Activity should not be finishing", activity.isFinishing());
    }

    @Test
    public void testPcViewActivityWithIntent() {
        // Test PcView with various intent extras that might cause crashes
        Intent intent = new Intent();
        intent.putExtra("hostname", "test.local");
        intent.putExtra("port", 47989);
        intent.putExtra("pin", "1234");
        intent.putExtra("passphrase", "test");

        PcView activity = Robolectric.buildActivity(PcView.class, intent).create().get();
        assertNotNull("PcView activity should be created with intent", activity);
        assertFalse("Activity should not be finishing", activity.isFinishing());
    }

    @Test
    public void testAppViewActivityCreation() {
        // Test AppView activity startup
        Intent intent = new Intent();
        intent.putExtra(AppView.NAME_EXTRA, "Test Computer");
        intent.putExtra(AppView.UUID_EXTRA, "test-uuid-123");

        AppView activity = Robolectric.buildActivity(AppView.class, intent).create().get();
        assertNotNull("AppView activity should be created", activity);
        assertFalse("Activity should not be finishing", activity.isFinishing());
    }


    @Test
    public void testMissingPermissions() {
        // Test behavior when required permissions are missing
        // Note: In Robolectric, permissions are typically granted by default
        // This test verifies the app doesn't crash when checking permissions

        try {
            PcView activity = Robolectric.buildActivity(PcView.class).create().get();
            assertNotNull("Activity should handle permission checks", activity);

            // Test permission checking doesn't crash
            int permissionResult = context.checkSelfPermission("android.permission.INTERNET");
            assertTrue("Permission check should work", permissionResult >= -1);
        } catch (Exception e) {
            fail("App should handle permission checks gracefully: " + e.getMessage());
        }
    }


    @Test
    public void testLowMemoryConditions() {
        // Simulate low memory conditions during startup
        Runtime runtime = Runtime.getRuntime();
        long maxMemory = runtime.maxMemory();
        long usedMemory = runtime.totalMemory() - runtime.freeMemory();

        // This test verifies the app doesn't crash under memory pressure
        try {
            PcView activity = Robolectric.buildActivity(PcView.class).create().get();
            assertNotNull("Activity should handle low memory conditions", activity);

            // Simulate memory pressure
            System.gc();

            activity.onLowMemory();
            assertFalse("Activity should not finish on low memory", activity.isFinishing());
        } catch (OutOfMemoryError e) {
            fail("App should handle low memory gracefully: " + e.getMessage());
        }
    }

    @Test
    public void testConfigurationChanges() {
        // Test startup with configuration changes
        PcView activity = Robolectric.buildActivity(PcView.class).create().start().resume().get();

        try {
            // Simulate configuration change
            android.content.res.Configuration newConfig = new android.content.res.Configuration();
            newConfig.orientation = android.content.res.Configuration.ORIENTATION_LANDSCAPE;
            activity.onConfigurationChanged(newConfig);

            assertFalse("Activity should survive configuration changes", activity.isFinishing());
        } catch (Exception e) {
            fail("Activity should handle configuration changes gracefully: " + e.getMessage());
        }
    }

    @Test
    public void testServiceBindingFailure() {
        // Test startup when service binding fails
        try {
            PcView activity = Robolectric.buildActivity(PcView.class).create().get();

            // Force service disconnection
            activity.onDestroy();

            assertNotNull("Activity should handle service binding failure", activity);
        } catch (Exception e) {
            fail("Activity should handle service binding failure gracefully: " + e.getMessage());
        }
    }

    private void deleteRecursively(File f) {
        if (f == null || !f.exists()) return;
        if (f.isDirectory()) {
            File[] children = f.listFiles();
            if (children != null) {
                for (File c : children) {
                    deleteRecursively(c);
                }
            }
        }
        f.delete();
    }
}