package com.limelight;

import org.junit.Test;
import java.nio.file.Files;
import java.nio.file.Path;
import static org.junit.Assert.*;

/** Static regression guards complement the device WebView/RPC test. */
public class KorriShellActivityLifecycleContractTest {
    @Test public void document_start_binding_precedes_navigation() throws Exception {
        String source = source();
        assertTrue(source.indexOf("WebViewCompat.addDocumentStartJavaScript(")
                < source.indexOf("webView.loadUrl("));
        assertFalse(source.contains("addJavascriptInterface("));
        assertFalse(source.contains("__korriInput"));
        assertFalse(source.contains("KorriNative"));
        assertTrue(source.contains("binding.consumed(sourceOrigin.toString(), mainFrame"));
    }

    @Test public void teardown_retires_the_webview_before_stopping_the_brain() throws Exception {
        String source = source().substring(source().indexOf("protected void onDestroy()"));
        assertTrue(source.indexOf("webView = null;") < source.indexOf("owned.destroy();"));
        assertTrue(source.indexOf("owned.destroy();") < source.indexOf("KorriBrainService.shutdown(this)"));
        assertTrue(source.contains("removeWebMessageListener"));
        assertTrue(source.contains("handler.removeCallbacks(startupDeadline)"));
    }

    @Test public void stream_client_does_not_require_external_storage_or_accessibility() throws Exception {
        String manifest = new String(Files.readAllBytes(Path.of("src/main/AndroidManifest.xml")), java.nio.charset.StandardCharsets.UTF_8);
        assertFalse(manifest.contains("MANAGE_EXTERNAL_STORAGE"));
        assertFalse(manifest.contains("BIND_ACCESSIBILITY_SERVICE"));
        assertFalse(manifest.contains("com.korri.retroarch"));
        assertTrue(source().contains("getFilesDir().getAbsolutePath()"));
    }

    private static String source() throws Exception {
        return new String(Files.readAllBytes(Path.of("src/main/java/com/limelight/KorriShellActivity.java")), java.nio.charset.StandardCharsets.UTF_8);
    }
}
