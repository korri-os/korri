package com.limelight.identity;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ResolveInfo;
import android.net.Uri;
import android.os.Looper;
import androidx.test.core.app.ApplicationProvider;
import org.json.JSONObject;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class KorriOwnerBindingHostTest {
    private Context context;
    private final List<Intent> launched = new ArrayList<>();
    private final Requests requests = new Requests();
    private final KorriOwnerBindingControllerTest.ConfigurableIdentity identity =
            new KorriOwnerBindingControllerTest.ConfigurableIdentity();

    @Before public void prepare() {
        context = ApplicationProvider.getApplicationContext();
        context.getSharedPreferences("korri-person-signer", Context.MODE_PRIVATE).edit().clear().commit();
    }
    private void installSigner() {
        ResolveInfo signer = new ResolveInfo();
        signer.activityInfo = new ActivityInfo();
        signer.activityInfo.packageName = "org.korri.signer.test";
        Intent query = new Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:"));
        query.addCategory(Intent.CATEGORY_BROWSABLE);
        Shadows.shadowOf(context.getPackageManager()).addResolveInfoForIntent(query, signer);
    }
    private KorriOwnerBindingHost open() {
        return new KorriOwnerBindingHost(context, identity, requests, launched::add);
    }
    @Test public void noIntentUntilKorridRequestsAndClosingStopsTheConsumer() throws Exception {
        installSigner();
        KorriOwnerBindingHost host = open();
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        assertEquals(0, launched.size());
        requests.pending = identity.ownerBindingTemplate(1);
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(250));
        assertEquals(1, launched.size());
        assertEquals("get_public_key", launched.get(0).getStringExtra("type"));
        assertEquals("Pending", new JSONObject(requests.snapshot).getJSONObject("personSigner").getString("_tag"));
        host.close();
        int consumed = requests.takes;
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(2));
        assertEquals(consumed, requests.takes);
        assertEquals(1, requests.detaches);
        host.close();
        assertEquals(1, requests.detaches);
    }
    @Test public void pendingResultSurvivesHostRecreationAndOldHostCannotApplyIt() throws Exception {
        installSigner();
        KorriOwnerBindingHost first = open();
        requests.pending = identity.ownerBindingTemplate(1);
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        Intent request = launched.get(0);
        first.close();
        launched.clear();
        try (KorriOwnerBindingHost replacement = open()) {
            assertEquals("Pending", new JSONObject(requests.snapshot).getJSONObject("personSigner").getString("_tag"));
            Intent rejected = new Intent().putExtra("id", request.getStringExtra("id")).putExtra("rejected", true);
            first.onActivityResult(Activity.RESULT_OK, rejected);
            assertEquals("Pending", new JSONObject(requests.snapshot).getJSONObject("personSigner").getString("_tag"));
            replacement.onActivityResult(Activity.RESULT_OK, rejected);
            assertEquals("Denied", new JSONObject(requests.snapshot).getJSONObject("personSigner").getString("_tag"));
            assertNull(identity.appliedEvent);
            assertTrue(launched.isEmpty());
        }
    }
    @Test public void unavailableSignerIsReportedWithoutCreatingAKeyOrLaunchingAnIntent() throws Exception {
        try (KorriOwnerBindingHost host = open()) {
            JSONObject status = new JSONObject(requests.snapshot);
            assertEquals("Unowned", status.getJSONObject("identity").getString("_tag"));
            assertTrue(status.getString("signerRequirement").contains("Amber"));
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertTrue(launched.isEmpty());
            assertNull(identity.appliedEvent);
        }
    }
    @Test public void wrongResultIsRejectedAndDoesNotCreateOwnership() throws Exception {
        installSigner();
        try (KorriOwnerBindingHost host = open()) {
            requests.pending = identity.ownerBindingTemplate(1);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            host.onActivityResult(Activity.RESULT_OK, new Intent().putExtra("id", "wrong"));
            assertEquals("InvalidResponse", new JSONObject(requests.snapshot).getJSONObject("personSigner").getString("_tag"));
            assertNull(identity.appliedEvent);
        }
    }
    private static final class Requests implements KorriOwnerBindingHost.NativeRequests {
        long generation;
        String snapshot;
        String pending;
        int takes;
        int detaches;
        @Override public long update(long expected, String value) {
            if (expected == 0) ++generation;
            if (expected == 0 || expected == generation) snapshot = value;
            return generation;
        }
        @Override public String take(long expected) {
            ++takes;
            if (expected != generation) return null;
            String next = pending;
            pending = null;
            return next;
        }
        @Override public void detach(long expected) { if (expected == generation) ++detaches; }
    }
}
