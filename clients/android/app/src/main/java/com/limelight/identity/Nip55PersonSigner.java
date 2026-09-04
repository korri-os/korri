package com.limelight.identity;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.database.Cursor;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.UUID;

/** Android NIP-55 adapter. Amber is the first named compatible signer, not a protocol dependency. */
public final class Nip55PersonSigner implements PersonSigner {
    static final long REQUEST_TIMEOUT_MS = 120_000L;
    private static final String PREFS = "korri-person-signer";
    private static final String SELECTED_PACKAGE = "selected-package";
    private static final String SELECTED_PUBLIC_KEY = "selected-public-key";
    private static final String PENDING_PHASE = "pending-phase";
    private static final String PENDING_ID = "pending-id";
    private static final String PENDING_TEMPLATE = "pending-template";
    private static final String PENDING_STARTED_AT = "pending-started-at";
    private static final String PHASE_ACCOUNT = "account";
    private static final String PHASE_SIGN = "sign";

    public interface IntentLauncher {
        void launch(Intent intent);
    }

    interface Clock {
        long nowMillis();
    }

    private final ContentResolver resolver;
    private final PackageManager packageManager;
    private final SharedPreferences preferences;
    private final IntentLauncher launcher;
    private final Listener listener;
    private final Clock clock;
    private final Handler handler;
    private final Runnable timeout = this::timeoutPendingRequest;
    private State state;

    public Nip55PersonSigner(
            Context context,
            IntentLauncher launcher,
            Listener listener) {
        this(
                context,
                context.getContentResolver(),
                context.getPackageManager(),
                context.getSharedPreferences(PREFS, Context.MODE_PRIVATE),
                launcher,
                listener,
                System::currentTimeMillis,
                new Handler(Looper.getMainLooper()));
    }

    Nip55PersonSigner(
            Context context,
            ContentResolver resolver,
            PackageManager packageManager,
            SharedPreferences preferences,
            IntentLauncher launcher,
            Listener listener,
            Clock clock,
            Handler handler) {
        this.resolver = resolver;
        this.packageManager = packageManager;
        this.preferences = preferences;
        this.launcher = launcher;
        this.listener = listener;
        this.clock = clock;
        this.handler = handler;
        if (hasPendingRequest()) {
            state = State.pending("Waiting for the selected signer");
            scheduleTimeout();
        } else {
            state = State.unavailable(isAvailable()
                    ? "Choose Set up owner to select a signer account"
                    : signerRequirement());
        }
    }

    @Override
    public State state() {
        return state;
    }

    @Override
    public boolean isAvailable() {
        return !compatibleSignerPackages().isEmpty();
    }

    @Override
    public State request(Request request) {
        if (hasPendingRequest()) return transition(State.pending("Waiting for the selected signer"));
        if (!isAvailable()) return transition(State.unavailable(signerRequirement()));
        String selectedPackage = preferences.getString(SELECTED_PACKAGE, null);
        String selectedPublicKey = preferences.getString(SELECTED_PUBLIC_KEY, null);
        if (selectedPackage == null
                || selectedPublicKey == null
                || !compatibleSignerPackages().contains(selectedPackage)) {
            clearSelection();
            return requestAccount(request.unsignedEventTemplate);
        }
        return requestSignature(
                request.unsignedEventTemplate,
                selectedPackage,
                selectedPublicKey,
                clock.nowMillis());
    }

    @Override
    public void onActivityResult(int resultCode, Intent data) {
        if (!hasPendingRequest()) return;
        if (resultCode != Activity.RESULT_OK) {
            clearPending();
            transition(State.defect("The signer did not complete the request"));
            return;
        }
        if (data == null) {
            clearPending();
            transition(State.invalidResponse("The signer returned no response"));
            return;
        }
        String expectedId = preferences.getString(PENDING_ID, null);
        if (expectedId == null || !expectedId.equals(data.getStringExtra("id"))) {
            clearPending();
            transition(State.invalidResponse("The signer returned the wrong request id"));
            return;
        }
        if (data.getBooleanExtra("rejected", false)) {
            clearPending();
            transition(State.denied());
            return;
        }
        String phase = preferences.getString(PENDING_PHASE, null);
        String template = preferences.getString(PENDING_TEMPLATE, null);
        long startedAt = preferences.getLong(PENDING_STARTED_AT, 0L);
        if (PHASE_ACCOUNT.equals(phase)) {
            String publicKey = data.getStringExtra("result");
            String signerPackage = data.getStringExtra("package");
            if (!validPublicKey(publicKey)
                    || signerPackage == null
                    || !compatibleSignerPackages().contains(signerPackage)) {
                clearPending();
                transition(State.invalidResponse("The signer returned an invalid account"));
                return;
            }
            preferences.edit()
                    .putString(SELECTED_PACKAGE, signerPackage)
                    .putString(SELECTED_PUBLIC_KEY, publicKey)
                    .apply();
            clearPending();
            requestSignature(template, signerPackage, publicKey, startedAt);
            return;
        }
        if (!PHASE_SIGN.equals(phase) || template == null) {
            clearPending();
            transition(State.invalidResponse("The signer response has no matching request"));
            return;
        }
        String event = data.getStringExtra("event");
        String ownerPublicKey = preferences.getString(SELECTED_PUBLIC_KEY, null);
        clearPending();
        if (event == null || event.isEmpty() || !validPublicKey(ownerPublicKey)) {
            transition(State.invalidResponse("The signer returned no signed event"));
            return;
        }
        transition(State.approved(ownerPublicKey, template, event));
    }

    private State requestAccount(String template) {
        long startedAt = clock.nowMillis();
        String id = UUID.randomUUID().toString();
        persistPending(PHASE_ACCOUNT, id, template, startedAt);
        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:"));
        intent.addCategory(Intent.CATEGORY_BROWSABLE);
        intent.putExtra("type", "get_public_key");
        intent.putExtra("id", id);
        JSONArray permissions = new JSONArray();
        JSONObject signEvent = new JSONObject();
        try {
            signEvent.put("type", "sign_event");
            signEvent.put("kind", 30_078);
            permissions.put(signEvent);
        } catch (Exception error) {
            clearPending();
            return transition(State.defect("Korri could not create signer permissions"));
        }
        intent.putExtra("permissions", permissions.toString());
        return launch(intent, "Choose a signer account");
    }

    private State requestSignature(
            String template,
            String signerPackage,
            String ownerPublicKey,
            long startedAt) {
        if (template == null || clock.nowMillis() - startedAt >= REQUEST_TIMEOUT_MS) {
            clearPending();
            return transition(State.defect("The signer request timed out"));
        }
        State resolverState = queryRememberedPermission(template, signerPackage, ownerPublicKey);
        if (resolverState != null) return transition(resolverState);

        String id = UUID.randomUUID().toString();
        persistPending(PHASE_SIGN, id, template, startedAt);
        Intent intent = new Intent(
                Intent.ACTION_VIEW,
                Uri.parse("nostrsigner:" + Uri.encode(template)));
        intent.addCategory(Intent.CATEGORY_BROWSABLE);
        intent.setPackage(signerPackage);
        intent.putExtra("type", "sign_event");
        intent.putExtra("id", id);
        intent.putExtra("current_user", ownerPublicKey);
        return launch(intent, "Approve the owner binding in your signer");
    }

    /** NIP-55 permits background resolver calls only after account permission selection. */
    private State queryRememberedPermission(
            String template,
            String signerPackage,
            String ownerPublicKey) {
        Uri uri = Uri.parse("content://" + signerPackage + ".SIGN_EVENT");
        try (Cursor cursor = resolver.query(
                uri,
                new String[] {template, "", ownerPublicKey},
                null,
                null,
                null)) {
            if (cursor == null) return null;
            if (cursor.getColumnIndex("rejected") >= 0) return State.denied();
            if (!cursor.moveToFirst()) return null;
            int eventColumn = cursor.getColumnIndex("event");
            if (eventColumn < 0) {
                return State.invalidResponse("The signer resolver returned no event column");
            }
            String event = cursor.getString(eventColumn);
            return event == null || event.isEmpty()
                    ? State.invalidResponse("The signer resolver returned no signed event")
                    : State.approved(ownerPublicKey, template, event);
        } catch (SecurityException ignored) {
            return null;
        } catch (RuntimeException error) {
            return State.defect("The signer resolver failed");
        }
    }

    private State launch(Intent intent, String pendingMessage) {
        try {
            launcher.launch(intent);
            scheduleTimeout();
            return transition(State.pending(pendingMessage));
        } catch (RuntimeException error) {
            clearPending();
            return transition(State.defect("Android could not open the selected signer"));
        }
    }

    private Set<String> compatibleSignerPackages() {
        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse("nostrsigner:"));
        intent.addCategory(Intent.CATEGORY_BROWSABLE);
        List<ResolveInfo> activities = packageManager.queryIntentActivities(
                intent, PackageManager.MATCH_DEFAULT_ONLY);
        Set<String> packages = new HashSet<>();
        for (ResolveInfo activity : activities) {
            if (activity.activityInfo != null && activity.activityInfo.packageName != null) {
                packages.add(activity.activityInfo.packageName);
            }
        }
        return packages;
    }

    private void persistPending(
            String phase,
            String id,
            String template,
            long startedAt) {
        preferences.edit()
                .putString(PENDING_PHASE, phase)
                .putString(PENDING_ID, id)
                .putString(PENDING_TEMPLATE, template)
                .putLong(PENDING_STARTED_AT, startedAt)
                .commit();
    }

    private boolean hasPendingRequest() {
        return preferences.contains(PENDING_PHASE)
                && preferences.contains(PENDING_ID)
                && preferences.contains(PENDING_TEMPLATE)
                && preferences.contains(PENDING_STARTED_AT);
    }

    private void scheduleTimeout() {
        handler.removeCallbacks(timeout);
        long remaining = REQUEST_TIMEOUT_MS
                - Math.max(0L, clock.nowMillis() - preferences.getLong(PENDING_STARTED_AT, 0L));
        if (remaining <= 0L) timeoutPendingRequest();
        else handler.postDelayed(timeout, remaining);
    }

    private void timeoutPendingRequest() {
        if (!hasPendingRequest()) return;
        if (clock.nowMillis() - preferences.getLong(PENDING_STARTED_AT, 0L)
                < REQUEST_TIMEOUT_MS) {
            scheduleTimeout();
            return;
        }
        clearPending();
        transition(State.defect("The signer request timed out"));
    }

    private void clearPending() {
        handler.removeCallbacks(timeout);
        preferences.edit()
                .remove(PENDING_PHASE)
                .remove(PENDING_ID)
                .remove(PENDING_TEMPLATE)
                .remove(PENDING_STARTED_AT)
                .commit();
    }

    private void clearSelection() {
        preferences.edit()
                .remove(SELECTED_PACKAGE)
                .remove(SELECTED_PUBLIC_KEY)
                .commit();
    }

    private State transition(State next) {
        state = next;
        listener.onPersonSignerState(next);
        return next;
    }

    static boolean validPublicKey(String value) {
        if (value == null || value.length() != 64) return false;
        for (int index = 0; index < value.length(); index++) {
            char c = value.charAt(index);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) return false;
        }
        return true;
    }

    static String signerRequirement() {
        return "Install a compatible NIP-55 signer such as Amber";
    }

    @Override
    public void close() {
        handler.removeCallbacks(timeout);
    }
}
