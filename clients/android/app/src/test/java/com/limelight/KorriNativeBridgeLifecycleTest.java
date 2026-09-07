package com.limelight;

import android.net.Uri;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriNativeBridgeLifecycleTest {
    private final KorriTrustedPortalWebViewPolicy policy = new KorriTrustedPortalWebViewPolicy();
    private final KorriNativeBridgeLifecycle lifecycle = new KorriNativeBridgeLifecycle();

    @Test
    public void initialTrustedPortalCallbacksPreserveExistingNativeBridge() {
        RecordingBridgeOperations operations = new RecordingBridgeOperations();
        Uri trustedPortal = Uri.parse(policy.portalUrl());

        lifecycle.installBeforeInitialLoad(trustedPortal, policy, operations);
        lifecycle.onMainFramePageStarted(trustedPortal, policy, operations);
        lifecycle.onMainFramePageFinished();

        assertEquals(Arrays.asList("add:KorriNative", "add:KorriRpc"), operations.calls);
    }

    @Test
    public void trustedPageStartedAndFinishedDoNotAddBridgeAfterInitialLoad() {
        RecordingBridgeOperations operations = new RecordingBridgeOperations();
        Uri trustedPortal = Uri.parse(policy.portalUrl());

        lifecycle.onMainFramePageStarted(trustedPortal, policy, operations);
        lifecycle.onMainFramePageFinished();

        assertTrue(operations.calls.isEmpty());
    }

    @Test
    public void untrustedMainFrameStartRemovesNativeBridgeWithoutReadding() {
        RecordingBridgeOperations operations = new RecordingBridgeOperations();

        lifecycle.installBeforeInitialLoad(Uri.parse(policy.portalUrl()), policy, operations);
        lifecycle.onMainFramePageStarted(Uri.parse("https://example.com/page"), policy, operations);
        lifecycle.onMainFramePageFinished();

        assertEquals(Arrays.asList("add:KorriNative", "add:KorriRpc",
                "remove:KorriNative", "remove:KorriRpc"), operations.calls);
    }

    @Test
    public void untrustedInitialDocumentReceivesNeitherBinding() {
        RecordingBridgeOperations operations = new RecordingBridgeOperations();
        lifecycle.installBeforeInitialLoad(Uri.parse("https://example.com/"), policy, operations);
        assertTrue(operations.calls.isEmpty());
    }

    @Test
    public void teardownRemovesBothBindings() {
        RecordingBridgeOperations operations = new RecordingBridgeOperations();
        lifecycle.removeJavascriptInterfaces(operations);
        assertEquals(Arrays.asList("remove:KorriNative", "remove:KorriRpc"), operations.calls);
    }

    @Test
    public void debugConfiguredPortalOriginControlsBridgeLifecycle() {
        KorriTrustedPortalWebViewPolicy debugPolicy =
                KorriTrustedPortalWebViewPolicy.forRuntime(true, "http://192.0.2.10:5173/portal/");
        RecordingBridgeOperations operations = new RecordingBridgeOperations();

        lifecycle.installBeforeInitialLoad(Uri.parse(debugPolicy.portalUrl()), debugPolicy, operations);
        lifecycle.onMainFramePageStarted(Uri.parse("http://192.0.2.10:5173/src/main.ts"),
                debugPolicy, operations);
        lifecycle.onMainFramePageStarted(Uri.parse("http://192.0.2.10:5174/src/main.ts"),
                debugPolicy, operations);
        lifecycle.onMainFramePageFinished();

        assertEquals(Arrays.asList("add:KorriNative", "add:KorriRpc",
                "remove:KorriNative", "remove:KorriRpc"), operations.calls);
    }

    private static final class RecordingBridgeOperations
            implements KorriNativeBridgeLifecycle.Operations {
        private final List<String> calls = new ArrayList<>();

        @Override
        public void addJavascriptInterface(String name) {
            calls.add("add:" + name);
        }

        @Override
        public void removeJavascriptInterface(String name) {
            calls.add("remove:" + name);
        }
    }
}
