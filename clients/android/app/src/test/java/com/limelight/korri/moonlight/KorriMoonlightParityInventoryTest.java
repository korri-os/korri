package com.limelight.korri.moonlight;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** Machine-checks that no control disappears between the old host and U6. */
@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightParityInventoryTest {
    @Test
    public void controllerChordCannotBypassKorriLifecycleAuthority() throws Exception {
        String controller = read("src/main/java/com/limelight/binding/input/ControllerHandler.java");
        assertFalse(controller.contains("Start+Back+LB+RB is the quit combo"));
        assertFalse(controller.contains("pendingExit"));
        assertFalse(controller.contains("activityContext.finish()"));
    }

    @Test
    public void disconnectLeavesHostRunningWhileConfirmedQuitUsesHostQuitPath() throws Exception {
        String game = read("src/main/java/com/limelight/Game.java");
        assertTrue(game.contains("public void disconnect() {\n        finish();\n    }"));
        assertTrue(game.contains("quitOnStop = true;\n            dialog.dismiss();\n            finish();"));
        assertTrue(game.contains("if (httpConn != null && quitOnStop)"));
        assertTrue(game.contains("httpConn.quitApp();"));
    }

    @Test
    public void runtimeSettingsMutexLivesForTheProcessAndPreservesTerminalState() throws Exception {
        String controlStream = read("src/main/jni/moonlight-core/moonlight-common-c/src/ControlStream.c");

        assertTrue(controlStream.contains(
                "static atomic_flag runtimeSettingsLockFlag = ATOMIC_FLAG_INIT;"));
        assertTrue(controlStream.contains(
                "atomic_flag_test_and_set_explicit(&runtimeSettingsLockFlag, memory_order_acquire)"));
        assertTrue(controlStream.contains(
                "atomic_flag_clear_explicit(&runtimeSettingsLockFlag, memory_order_release)"));
        assertFalse(controlStream.contains("PltCreateMutex(&runtimeSettingsMutex);"));
        assertFalse(controlStream.contains("PltDeleteMutex(&runtimeSettingsMutex);"));
        assertTrue(controlStream.contains(
                "if (atomic_load_explicit(&runtimeSettingsPublished, memory_order_acquire)) {\n"
                        + "        SsRuntimeSettingsDispatchEndSession(&runtimeSettingsDispatch);\n"
                        + "    }"));
        assertTrue(controlStream.contains(
                "endRuntimeSettingsSession();\n    PltDeleteMutex(&enetMutex);"));
        assertTrue(controlStream.contains(
                "return SsRuntimeSettingsDispatchRequest(&runtimeSettingsDispatch,"));
        assertTrue(controlStream.contains(
                "SsRuntimeSettingsDispatchGetSnapshot(&runtimeSettingsDispatch, snapshot);"));
    }

    private static String read(String path) throws Exception {
        return new String(Files.readAllBytes(Path.of(path)), StandardCharsets.UTF_8);
    }
}
