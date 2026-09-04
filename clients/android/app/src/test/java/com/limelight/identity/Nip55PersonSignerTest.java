package com.limelight.identity;

import android.app.Activity;
import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ActivityInfo;
import android.content.pm.ResolveInfo;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;

import androidx.test.core.app.ApplicationProvider;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class Nip55PersonSignerTest {
    private static final String SIGNER_PACKAGE = "org.korri.signer.test";
    private static final String OWNER = "11".repeat(32);
    private static final String TEMPLATE = "{\"kind\":30078,\"created_at\":1,\"tags\":[],\"content\":\"\"}";
    private Context context;
    private SharedPreferences preferences;
    private RecordingLauncher launcher;
    private RecordingListener listener;
    private MutableClock clock;

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
        preferences = context.getSharedPreferences("korri-person-signer", Context.MODE_PRIVATE);
        preferences.edit().clear().commit();
        launcher = new RecordingLauncher();
        listener = new RecordingListener();
        clock = new MutableClock(10_000L);
        ResolveInfo signer = new ResolveInfo();
        signer.activityInfo = new ActivityInfo();
        signer.activityInfo.packageName = SIGNER_PACKAGE;
        Intent query = new Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:"));
        query.addCategory(Intent.CATEGORY_BROWSABLE);
        Shadows.shadowOf(context.getPackageManager()).addResolveInfoForIntent(query, signer);
    }

    @Test
    public void selectsAnAccountThenTargetsThatExactSignerPackage() {
        Nip55PersonSigner signer = signer();
        assertTrue(signer.isAvailable());

        assertEquals(PersonSigner.Kind.Pending,
                signer.request(new PersonSigner.Request(TEMPLATE)).kind);
        Intent account = launcher.last();
        assertNull(account.getPackage());
        assertEquals("get_public_key", account.getStringExtra("type"));
        assertTrue(account.getStringExtra("permissions").contains("\"kind\":30078"));

        Intent selected = new Intent()
                .putExtra("id", account.getStringExtra("id"))
                .putExtra("result", OWNER)
                .putExtra("package", SIGNER_PACKAGE);
        signer.onActivityResult(Activity.RESULT_OK, selected);

        Intent signing = launcher.last();
        assertEquals(SIGNER_PACKAGE, signing.getPackage());
        assertEquals("sign_event", signing.getStringExtra("type"));
        assertEquals(OWNER, signing.getStringExtra("current_user"));
        assertEquals(TEMPLATE, signing.getData().getSchemeSpecificPart());
    }

    @Test
    public void pendingRequestSurvivesActivityAndProcessRecreation() {
        Nip55PersonSigner first = signer();
        first.request(new PersonSigner.Request(TEMPLATE));
        Intent account = launcher.last();
        first.close();

        RecordingLauncher replacementLauncher = new RecordingLauncher();
        Nip55PersonSigner replacement = new Nip55PersonSigner(
                context,
                context.getContentResolver(),
                context.getPackageManager(),
                preferences,
                replacementLauncher,
                listener,
                clock,
                new Handler(Looper.getMainLooper()));
        assertEquals(PersonSigner.Kind.Pending, replacement.state().kind);
        assertTrue(replacementLauncher.intents.isEmpty());

        replacement.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", account.getStringExtra("id"))
                .putExtra("result", OWNER)
                .putExtra("package", SIGNER_PACKAGE));
        assertEquals(SIGNER_PACKAGE, replacementLauncher.last().getPackage());
    }

    @Test
    public void denialAndMalformedResultsAreDifferentStates() {
        Nip55PersonSigner signer = signer();
        signer.request(new PersonSigner.Request(TEMPLATE));
        String accountId = launcher.last().getStringExtra("id");
        signer.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", accountId)
                .putExtra("rejected", true));
        assertEquals(PersonSigner.Kind.Denied, signer.state().kind);

        signer.request(new PersonSigner.Request(TEMPLATE));
        Intent account = launcher.last();
        signer.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", account.getStringExtra("id"))
                .putExtra("result", OWNER)
                .putExtra("package", SIGNER_PACKAGE));
        Intent signing = launcher.last();
        signer.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", signing.getStringExtra("id")));
        assertEquals(PersonSigner.Kind.InvalidResponse, signer.state().kind);
    }

    @Test
    public void selectedAccountUsesContentResolverBeforeOpeningAnActivity() {
        Nip55PersonSigner signer = signer();
        signer.request(new PersonSigner.Request(TEMPLATE));
        Intent account = launcher.last();
        signer.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", account.getStringExtra("id"))
                .putExtra("result", OWNER)
                .putExtra("package", SIGNER_PACKAGE));
        Intent signing = launcher.last();
        signer.onActivityResult(Activity.RESULT_OK, new Intent()
                .putExtra("id", signing.getStringExtra("id"))
                .putExtra("rejected", true));

        ConfigurableSignerProvider.event = "{\"signed\":true}";
        org.robolectric.Robolectric.setupContentProvider(
                ConfigurableSignerProvider.class,
                SIGNER_PACKAGE + ".SIGN_EVENT");
        int launchesBefore = launcher.intents.size();
        PersonSigner.State result = signer.request(new PersonSigner.Request(TEMPLATE));
        assertEquals(PersonSigner.Kind.Approved, result.kind);
        assertEquals("{\"signed\":true}", result.signedEventJson);
        assertEquals(launchesBefore, launcher.intents.size());
        assertEquals(TEMPLATE, ConfigurableSignerProvider.lastProjection[0]);
        assertEquals(OWNER, ConfigurableSignerProvider.lastProjection[2]);
    }

    @Test
    public void requestHasOneBoundedDeadlineAcrossAccountAndSigningPhases() {
        Nip55PersonSigner signer = signer();
        signer.request(new PersonSigner.Request(TEMPLATE));
        clock.now += Nip55PersonSigner.REQUEST_TIMEOUT_MS;
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(
                Duration.ofMillis(Nip55PersonSigner.REQUEST_TIMEOUT_MS));
        assertEquals(PersonSigner.Kind.Defect, signer.state().kind);
        assertFalse(preferences.contains("pending-phase"));
    }

    private Nip55PersonSigner signer() {
        return new Nip55PersonSigner(
                context,
                context.getContentResolver(),
                context.getPackageManager(),
                preferences,
                launcher,
                listener,
                clock,
                new Handler(Looper.getMainLooper()));
    }

    static final class RecordingLauncher implements Nip55PersonSigner.IntentLauncher {
        final List<Intent> intents = new ArrayList<>();

        @Override
        public void launch(Intent intent) {
            intents.add(intent);
        }

        Intent last() {
            return intents.get(intents.size() - 1);
        }
    }

    static final class RecordingListener implements PersonSigner.Listener {
        PersonSigner.State last;

        @Override
        public void onPersonSignerState(PersonSigner.State state) {
            last = state;
        }
    }

    static final class MutableClock implements Nip55PersonSigner.Clock {
        long now;

        MutableClock(long now) {
            this.now = now;
        }

        @Override
        public long nowMillis() {
            return now;
        }
    }

    public static final class ConfigurableSignerProvider extends ContentProvider {
        static String event;
        static String[] lastProjection;

        @Override
        public boolean onCreate() {
            return true;
        }

        @Override
        public Cursor query(Uri uri, String[] projection, String selection,
                String[] selectionArgs, String sortOrder) {
            lastProjection = projection;
            MatrixCursor cursor = new MatrixCursor(new String[] {"result", "event"});
            cursor.addRow(new Object[] {"signature", event});
            return cursor;
        }

        @Override public String getType(Uri uri) { return null; }
        @Override public Uri insert(Uri uri, ContentValues values) { return null; }
        @Override public int delete(Uri uri, String selection, String[] selectionArgs) { return 0; }
        @Override public int update(Uri uri, ContentValues values, String selection,
                String[] selectionArgs) { return 0; }
    }
}
