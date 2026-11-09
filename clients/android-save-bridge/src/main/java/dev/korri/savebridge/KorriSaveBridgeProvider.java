package dev.korri.savebridge;

import android.app.Activity;
import android.app.Application;
import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.pm.PackageManager;
import android.content.pm.ProviderInfo;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.ParcelFileDescriptor;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;

/**
 * ADR 0054, case 2: the Korri save bridge, built into every APK that Korri
 * builds. Korri copies save folders in before a launch and out after End, and
 * asks the game to quit on End. The game itself keeps plain file paths in its
 * own storage; nothing in its engine changes.
 *
 * Only Korri can call this provider: it is guarded by a signature-level
 * permission, so the caller must be signed with the same key as the game APK,
 * which is Korri's key.
 *
 * Folders are declared in the game APK's own manifest, in its native format:
 * {@code <meta-data android:name="dev.korri.savebridge.folders"
 *  android:value="save=v190/save" />}, paths relative to
 * {@code getExternalFilesDir(null)}, or to {@code getFilesDir()} with an
 * {@code internal:} prefix. Keys match the plugin's saveLocations keys.
 */
public final class KorriSaveBridgeProvider extends ContentProvider {
    public static final String FOLDERS = "dev.korri.savebridge.folders";
    public static final String METHOD_LIST = "list";
    public static final String METHOD_CLEAR = "clear";
    public static final String METHOD_QUIT = "quit";

    private final Map<String, File> folders = new HashMap<>();

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public void attachInfo(android.content.Context context, ProviderInfo info) {
        super.attachInfo(context, info);
        // Providers attach before Application.onCreate, so this sees every activity.
        ((Application) context.getApplicationContext())
                .registerActivityLifecycleCallbacks(new Activities());
        try {
            Bundle meta = context.getPackageManager()
                    .getApplicationInfo(context.getPackageName(), PackageManager.GET_META_DATA).metaData;
            String spec = meta == null ? null : meta.getString(FOLDERS);
            for (String entry : spec == null ? new String[0] : spec.split(";")) {
                String[] kv = entry.split("=", 2);
                if (kv.length != 2 || !Paths.validKey(kv[0])) {
                    continue;
                }
                // "internal:" paths are under getFilesDir(); others under
                // getExternalFilesDir(null).
                boolean internal = kv[1].startsWith("internal:");
                String relative = internal ? kv[1].substring("internal:".length()) : kv[1];
                if (Paths.validRelative(relative)) {
                    File base = internal ? context.getFilesDir() : context.getExternalFilesDir(null);
                    folders.put(kv[0], new File(base, relative));
                }
            }
        } catch (PackageManager.NameNotFoundException e) {
            throw new IllegalStateException(e);
        }
    }

    private File folder(String key) throws FileNotFoundException {
        File folder = folders.get(key);
        if (folder == null) {
            throw new FileNotFoundException("no save folder " + key);
        }
        return folder;
    }

    /** content://AUTHORITY/KEY/relative/path */
    private File file(Uri uri) throws FileNotFoundException {
        java.util.List<String> parts = uri.getPathSegments();
        if (parts.size() < 2) {
            throw new FileNotFoundException("no file named");
        }
        String relative = String.join("/", parts.subList(1, parts.size()));
        if (!Paths.validRelative(relative)) {
            throw new FileNotFoundException("invalid path");
        }
        return new File(folder(parts.get(0)), relative);
    }

    @Override
    public Bundle call(String method, String key, Bundle extras) {
        Bundle out = new Bundle();
        try {
            switch (method) {
                case METHOD_LIST: {
                    ArrayList<String> files = new ArrayList<>();
                    Paths.list(folder(key), "", files);
                    out.putStringArrayList("files", files);
                    break;
                }
                case METHOD_CLEAR:
                    Paths.deleteChildren(folder(key));
                    folder(key).mkdirs();
                    break;
                case METHOD_QUIT:
                    quit();
                    break;
                default:
                    throw new IllegalArgumentException("unknown method " + method);
            }
            out.putBoolean("ok", true);
        } catch (IOException e) {
            out.putBoolean("ok", false);
            out.putString("error", e.getMessage());
        }
        return out;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        File target = file(uri);
        if (mode.contains("w")) {
            target.getParentFile().mkdirs();
            return ParcelFileDescriptor.open(target, ParcelFileDescriptor.MODE_WRITE_ONLY
                    | ParcelFileDescriptor.MODE_CREATE | ParcelFileDescriptor.MODE_TRUNCATE);
        }
        return ParcelFileDescriptor.open(target, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    /** End: finish every activity, then let the process exit. The engine
     *  flushes saves on its normal shutdown path. */
    private void quit() {
        new Handler(Looper.getMainLooper()).post(() -> {
            for (Activity activity : Activities.live()) {
                activity.finishAndRemoveTask();
            }
        });
    }

    @Override public Cursor query(Uri u, String[] p, String s, String[] a, String o) { return null; }
    @Override public String getType(Uri uri) { return "application/octet-stream"; }
    @Override public Uri insert(Uri uri, ContentValues values) { return null; }
    @Override public int delete(Uri uri, String s, String[] a) { return 0; }
    @Override public int update(Uri uri, ContentValues v, String s, String[] a) { return 0; }

    /** Tracks live activities so quit can finish them. Registered by the provider. */
    static final class Activities implements Application.ActivityLifecycleCallbacks {
        private static final java.util.Set<Activity> LIVE =
                java.util.Collections.newSetFromMap(new java.util.WeakHashMap<>());
        static java.util.List<Activity> live() {
            synchronized (LIVE) { return new ArrayList<>(LIVE); }
        }
        @Override public void onActivityCreated(Activity a, Bundle b) { synchronized (LIVE) { LIVE.add(a); } }
        @Override public void onActivityDestroyed(Activity a) { synchronized (LIVE) { LIVE.remove(a); } }
        @Override public void onActivityStarted(Activity a) {}
        @Override public void onActivityResumed(Activity a) {}
        @Override public void onActivityPaused(Activity a) {}
        @Override public void onActivityStopped(Activity a) {}
        @Override public void onActivitySaveInstanceState(Activity a, Bundle b) {}
    }

}
