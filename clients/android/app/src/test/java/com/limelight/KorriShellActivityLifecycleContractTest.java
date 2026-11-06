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

    @Test public void stream_client_does_not_require_external_storage_or_accessibility() throws Exception {
        String manifest = manifest();
        assertFalse(manifest.contains("MANAGE_EXTERNAL_STORAGE"));
        assertFalse(manifest.contains("BIND_ACCESSIBILITY_SERVICE"));
        assertFalse(manifest.contains("com.korri.retroarch"));
        assertTrue(source().contains("getFilesDir().getAbsolutePath()"));
    }

    private static String manifest() throws Exception {
        return new String(Files.readAllBytes(Path.of("src/main/AndroidManifest.xml")), java.nio.charset.StandardCharsets.UTF_8);
    }
    private static String source() throws Exception {
        return new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/KorriShellActivity.java")), java.nio.charset.StandardCharsets.UTF_8);
    }
}
