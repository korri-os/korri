package com.limelight;

import org.junit.Test;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class KorriShellActivityLifecycleContractTest {
    @Test
    public void foregroundShellKeepsItsWindowAwakeWithoutPersistentPowerMutation() throws Exception {
        String source = source();
        String onCreate = method(source, "protected void onCreate(Bundle savedInstanceState)",
                "private boolean notificationsAllowed()");

        assertTrue(source.contains("import android.view.WindowManager;"));
        assertTrue(onCreate.contains(
                "getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);"));
        assertFalse(source.contains("PowerManager"));
        assertFalse(source.contains("WakeLock"));
        assertFalse(source.contains("Settings.System.put"));
        assertFalse(source.contains("Settings.Global.put"));
        assertFalse(source.contains("Settings.Secure.put"));
        assertFalse(source.contains("SCREEN_OFF_TIMEOUT"));
        assertFalse(source.contains("STAY_ON_WHILE_PLUGGED_IN"));
    }

    @Test
    public void finishedShellRevokesAndDestroysItsOwnedPortalOnMainThread() throws Exception {
        String source = source();
        String onDestroy = method(source, "protected void onDestroy()", "protected void onResume()");

        assertTrue(onDestroy.contains("Looper.myLooper() != Looper.getMainLooper()"));
        assertOrdered(onDestroy,
                "final WebView ownedWebView = webView;",
                "webView = null;",
                "((ViewGroup) parent).removeView(ownedWebView);",
                "ownedWebView.removeJavascriptInterface(\"KorriNative\");",
                "ownedWebView.stopLoading();",
                "ownedWebView.destroy();",
                "super.onDestroy();");
        assertFalse(onDestroy.contains("KorriBrainService.stop"));
    }

    @Test
    public void localPublicationEvidenceFollowsAndroidStartAndRustPublication() throws Exception {
        String source = source();
        String launchLocal = method(source, "public String launchLocal(String specJson)",
                "private void requestAllFilesAccess()");

        assertOrdered(launchLocal,
                "startActivityOnUiThread(intent, \"local launcher start timed out\")",
                "KorriBrainService.publishLocalActiveLaunch(spec, specJson)",
                "Log.i(KorriLocalLaunchLifecycle.TAG",
                "KorriLocalLaunchLifecycle.published(");
        assertTrue(launchLocal.contains("active.launchId()"));
        assertTrue(launchLocal.contains("active.gameId()"));
        assertTrue(launchLocal.contains("spec.component.getPackageName()"));
        assertTrue(launchLocal.contains("spec.launcherId"));
        assertFalse(launchLocal.contains("Log.i(KorriLocalLaunchLifecycle.TAG, specJson"));

        String eventSource = new String(Files.readAllBytes(
                Path.of("src/main/java/com/limelight/KorriLocalLaunchLifecycle.java")),
                StandardCharsets.UTF_8);
        assertTrue(eventSource.contains("launchId="));
        assertTrue(eventSource.contains(" event=published"));
        assertTrue(eventSource.contains(" gameId="));
        assertTrue(eventSource.contains(" package="));
        assertTrue(eventSource.contains(" launcher="));
        assertFalse(eventSource.contains("specJson"));
        assertFalse(eventSource.contains("capability"));
        assertFalse(eventSource.contains("authorization"));
        assertFalse(eventSource.contains("controlToken"));
        assertFalse(eventSource.contains("controlPort"));
    }

    @Test
    public void streamAppDiscoveryReturnsCacheAndOnlySchedulesNetworkRefresh() throws Exception {
        String source = source();
        String query = method(source, "public String queryStreamApps(String hostUuid)",
                "/**\n         * JSON-encoded StartStreamResult");
        String cache = method(source,
                "private static List<NvApp> cachedAppList(", "\n    }\n}");

        assertOrdered(query,
                "discovery.query(hostUuid)",
                "for (NvApp app : current)");
        assertFalse(query.contains("awaitBinder("));
        assertFalse(query.contains("awaitOnlineComputer("));
        assertFalse(query.contains("repairAndLoadApps("));
        assertFalse(query.contains("new NvHTTP"));
        assertFalse(query.contains("ServerHelper.createStartIntent"));
        assertTrue(cache.contains("hasCachedAppList(context, hostUuid)"));
        assertTrue(cache.contains("java.util.Collections.emptyList()"));
    }

    @Test
    public void completedBackgroundAppRepairSignalsTheLivePortalOnly() throws Exception {
        String source = source();
        String install = method(source,
                "private synchronized void installMoonlightDiscovery(",
                "private void notifyStreamAppsChanged()");
        String notify = method(source,
                "private void notifyStreamAppsChanged()",
                "private KorriMoonlightShellFlow moonlightFlow(");
        String onDestroy = method(source, "protected void onDestroy()",
                "protected void onResume()");

        assertTrue(install.contains("getApplicationContext()"));
        assertTrue(install.contains("WeakReference<KorriShellActivity>"));
        assertTrue(install.contains("provisioning.repairAndLoadApps(hostUuid, guard)"));
        assertTrue(notify.contains("if (destroyed || webView == null) return;"));
        assertTrue(notify.contains("korri-stream-apps-changed"));
        assertOrdered(onDestroy,
                "clearMoonlightDiscovery()",
                "unbindService(serviceConnection)",
                "managerBinder = null");
    }

    @Test
    public void streamLaunchDelegatesToTheExecutableProvisioningFlow() throws Exception {
        String source = source();
        String startStream = method(source, "public String startStream(String specJson)",
                "private String queryFailed(Exception e)");
        String flow = method(source,
                "private KorriMoonlightShellFlow moonlightFlow(",
                "private ComputerManagerService.ComputerManagerBinder awaitBinder(int seconds)");

        assertOrdered(startStream,
                "awaitBinder(10)",
                "moonlightFlow(",
                "ServerHelper.createStartIntent(",
                ").startStream(specJson)",
                "return \"{\\\"_tag\\\":\\\"StreamStarted\\\"}\"");
        assertTrue(startStream.contains("catch (KorriMoonlightShellFlow.Failure error)"));
        assertFalse(startStream.contains("PairingManager.PairState"));
        assertFalse(startStream.contains("streamFailed(\"NotPaired\""));
        assertTrue(flow.contains("exactProvisioning::repairAndLoadApps"));
        assertTrue(flow.contains("reserveMoonlightActiveLaunch"));
        assertTrue(flow.contains("clearActiveLaunch"));
        assertTrue(flow.contains("gameStarter"));
    }

    @Test
    public void failedNativeMoonlightPublicationClearsOnlyTheAttemptedLaunch() throws Exception {
        String service = new String(Files.readAllBytes(Path.of(
                "src/main/java/com/simonwjackson/korri/korrid/KorriBrainService.java")),
                StandardCharsets.UTF_8);
        String reserve = method(service,
                "public static synchronized KorriActiveLaunch reserveMoonlightActiveLaunch(",
                "private static KorriActiveLaunch installActiveLaunch(");
        assertOrdered(reserve,
                "if (activeLaunch != null) return null;",
                "publishMoonlightActiveLaunch(",
                "KorridServer.clearActiveLaunch(launchId)",
                "throw error;");
    }

    @Test
    public void jniUsesTheProvenDeadlineThatOutlivesTheWholeBroker() throws Exception {
        String androidRust = new String(Files.readAllBytes(
                Path.of("../../../services/korrid/src/android.rs")),
                StandardCharsets.UTF_8);
        String upstreams = new String(Files.readAllBytes(
                Path.of("../../../services/korrid/src/upstreams.rs")),
                StandardCharsets.UTF_8);

        assertTrue(androidRust.contains("MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT"));
        assertTrue(upstreams.contains("MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT"));
        assertTrue(upstreams.contains(
                "timeout(MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT, async"));
        assertTrue(upstreams.contains(
                "MOONLIGHT_CERTIFICATE_CALLER_TIMEOUT > MOONLIGHT_CERTIFICATE_BROKER_TIMEOUT"));
    }

    @Test
    public void firstInstallBootstrapsArtemisHostsFromKorridNativePeers() throws Exception {
        String source = source();
        String install = method(source,
                "private synchronized void installMoonlightDiscovery(",
                "private void notifyStreamAppsChanged()");
        String clear = method(source,
                "private synchronized void clearMoonlightDiscovery()",
                "private synchronized void installMoonlightDiscovery(");
        String onResume = method(source, "protected void onResume()",
                "public void onWindowFocusChanged(boolean hasFocus)");

        assertOrdered(install,
                "KorridServer.moonlightHostCandidates()",
                "details.manualAddress = candidate.manualAddress",
                "binder.addComputerBlocking(details)",
                "moonlightHostBootstrap.start()");
        assertTrue(onResume.contains("moonlightHostBootstrap"));
        assertTrue(onResume.contains("bootstrap.start()"));
        assertTrue(clear.contains("ownedBootstrap.close()"));
        assertFalse(install.contains("PairingManager"));
        assertFalse(install.contains("SpinnerDialog"));
        assertFalse(install.contains("Intent"));
    }

    @Test
    public void serviceBindingIsReleasedByBindingOwnershipRatherThanCallbackTiming() throws Exception {
        String source = source();
        String onCreate = method(source, "protected void onCreate(Bundle savedInstanceState)",
                "private boolean notificationsAllowed()");
        String onDestroy = method(source, "protected void onDestroy()", "protected void onResume()");

        assertTrue(source.contains("private boolean computerManagerBound;"));
        assertTrue(onCreate.contains("computerManagerBound = bindService("));
        assertTrue(onDestroy.contains("clearMoonlightDiscovery()"));
        assertTrue(onDestroy.contains("if (computerManagerBound)"));
        assertOrdered(onDestroy,
                "computerManagerBound = false;",
                "unbindService(serviceConnection);",
                "managerBinder = null;",
                "super.onDestroy();");
        assertFalse(onDestroy.contains("if (managerBinder != null)"));
    }

    @Test
    public void shellTakesTheWholeScreenAndReclaimsItWhenFocusReturns() throws Exception {
        String source = source();
        String onCreate = method(source, "protected void onCreate(Bundle savedInstanceState)",
                "private void applyImmersiveFullscreen()");
        String applyImmersive = method(source, "private void applyImmersiveFullscreen()",
                "private boolean notificationsAllowed()");
        String onResume = method(source, "protected void onResume()",
                "public void onWindowFocusChanged(boolean hasFocus)");
        String onWindowFocusChanged = method(source,
                "public void onWindowFocusChanged(boolean hasFocus)",
                "protected void onActivityResult(");

        assertTrue(source.contains("import androidx.core.view.WindowCompat;"));
        assertTrue(source.contains("import androidx.core.view.WindowInsetsCompat;"));
        assertTrue(source.contains("import androidx.core.view.WindowInsetsControllerCompat;"));

        assertTrue(applyImmersive.contains(
                "WindowCompat.setDecorFitsSystemWindows(getWindow(), false);"));
        assertTrue(applyImmersive.contains(
                "WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE"));
        assertTrue(applyImmersive.contains("hide(WindowInsetsCompat.Type.systemBars());"));

        assertTrue(onCreate.contains("applyImmersiveFullscreen();"));
        assertTrue(onResume.contains("applyImmersiveFullscreen();"));
        assertTrue(onWindowFocusChanged.contains("if (hasFocus)"));
        assertTrue(onWindowFocusChanged.contains("applyImmersiveFullscreen();"));

        assertFalse(source.contains("FLAG_FULLSCREEN"));
        assertFalse(source.contains("SYSTEM_UI_FLAG"));
    }

    private static String source() throws Exception {
        return new String(Files.readAllBytes(
                Path.of("src/main/java/com/limelight/KorriShellActivity.java")),
                StandardCharsets.UTF_8);
    }

    private static String method(String source, String startNeedle, String endNeedle) {
        int start = source.indexOf(startNeedle);
        int end = source.indexOf(endNeedle, start + startNeedle.length());
        assertTrue("missing method start: " + startNeedle, start >= 0);
        assertTrue("missing method end: " + endNeedle, end > start);
        return source.substring(start, end);
    }

    private static void assertOrdered(String source, String... needles) {
        int previous = -1;
        for (String needle : needles) {
            int next = source.indexOf(needle);
            assertTrue("missing lifecycle step: " + needle, next >= 0);
            assertTrue("out-of-order lifecycle step: " + needle, next > previous);
            previous = next;
        }
    }
}
