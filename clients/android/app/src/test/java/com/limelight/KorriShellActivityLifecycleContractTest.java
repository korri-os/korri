package com.limelight;

import org.junit.Test;
import java.nio.file.Files;
import java.nio.file.Path;
import static org.junit.Assert.*;

/** Source guards complement the real NativeActivity lifecycle device test. */
public class KorriShellActivityLifecycleContractTest {
    @Test public void native_bootstrap_precedes_native_activity_thread() throws Exception {
        String source = source();
        assertTrue(source.contains("extends NativeActivity"));
        assertTrue(source.indexOf("KorriBrainService.ensureRunning(") < source.indexOf("super.onCreate(savedInstanceState)"));
        assertFalse(source.contains("new WebView("));
        assertFalse(source.contains("putExtra("));
    }

    @Test public void activity_does_not_stop_the_service_on_recreation_or_background() throws Exception {
        assertFalse(source().contains("KorriBrainService.shutdown("));
        String manifest = manifest();
        assertTrue(manifest.contains("android.app.lib_name\" android:value=\"korri_shell"));
        assertTrue(manifest.contains("orientation|screenSize|smallestScreenSize"));
    }

    @Test public void stream_client_does_not_require_external_storage() throws Exception {
        String manifest = manifest();
        assertFalse(manifest.contains("MANAGE_EXTERNAL_STORAGE"));
        assertFalse(manifest.contains("com.korri.retroarch"));
        assertTrue(source().contains("getFilesDir().getAbsolutePath()"));
    }

    /** ADR 0054: one accessibility service, for the Guide button only, that
     *  reads no window content. */
    @Test public void the_only_accessibility_service_reads_keys_not_windows() throws Exception {
        String manifest = manifest();
        assertTrue(manifest.split("BIND_ACCESSIBILITY_SERVICE", -1).length == 2);
        assertTrue(manifest.contains("KorriGuideService"));
        String config = new String(java.nio.file.Files.readAllBytes(
                java.nio.file.Paths.get("src/main/res/xml/korri_guide_service.xml")));
        assertTrue(config.contains("android:canRetrieveWindowContent=\"false\""));
        assertTrue(config.contains("flagRequestFilterKeyEvents"));
    }

    @Test public void cutout_and_system_bars_use_the_full_window_on_every_resume() throws Exception {
        String source = source();
        assertTrue(source.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS"));
        assertTrue(source.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES"));
        assertTrue(source.contains("WindowCompat.setDecorFitsSystemWindows(getWindow(), false)"));
        assertTrue(source.contains("insets.hide(WindowInsetsCompat.Type.systemBars())"));
        assertTrue(source.contains("if (focused) applyFullscreen()"));
    }

    @Test public void back_stays_in_the_native_semantic_input_path() throws Exception {
        String activity = manifest().split("android:name=\".KorriShellActivity\"")[1].split("</activity>")[0];
        assertTrue(activity.contains("android:enableOnBackInvokedCallback=\"false\""));
        assertTrue(source().contains("@Override public void onBackPressed() { }"));
        assertFalse(source().contains("finish()"));
    }

    @Test public void input_device_removal_is_observed_and_listener_is_retired() throws Exception {
        String source = source();
        assertTrue(source.contains("registerInputDeviceListener(inputDevices, null)"));
        assertTrue(source.contains("onInputDeviceRemoved(int id) { removedInputDevices.add(id); }"));
        assertTrue(source.contains("unregisterInputDeviceListener(inputDevices)"));
    }

    private static String manifest() throws Exception {
        return new String(Files.readAllBytes(Path.of("src/main/AndroidManifest.xml")), java.nio.charset.StandardCharsets.UTF_8);
    }
    private static String source() throws Exception {
        return new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/KorriShellActivity.java")), java.nio.charset.StandardCharsets.UTF_8);
    }
}
