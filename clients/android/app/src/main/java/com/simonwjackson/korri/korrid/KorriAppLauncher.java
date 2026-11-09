package com.simonwjackson.korri.korrid;

import android.app.usage.UsageEvents;
import android.app.usage.UsageStatsManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import androidx.core.content.FileProvider;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.util.List;

/**
 * ADR 0054: platform calls for games that run on this device. korrid decides
 * what to start and holds the live session; this class only performs the
 * Android calls Rust asks for, and reports what Android observes.
 */
public final class KorriAppLauncher {
    private static final String TAG = "KorriAppLauncher";
    private static final long USAGE_POLL_MS = 1000;
    private static volatile Context context;
    private static volatile String watched;
    private static final Handler handler = new Handler(Looper.getMainLooper());

    private KorriAppLauncher() {}

    public static void initialize(Context app) {
        context = app.getApplicationContext();
    }

    // ---- Called by Rust (android/app_launcher.rs). ----

    public static String isInstalled(String packageName) {
        try {
            context.getPackageManager().getPackageInfo(packageName, 0);
            return "yes";
        } catch (PackageManager.NameNotFoundException e) {
            return "no";
        }
    }

    /** Start a validated plan: package, optional activity, action and content grant. */
    public static String start(String planJson, String contentPath, String savesJson) {
        // Android blocks activity starts from a background app without an
        // error, so refuse here instead of recording a game that never opened.
        android.app.ActivityManager.RunningAppProcessInfo self =
                new android.app.ActivityManager.RunningAppProcessInfo();
        android.app.ActivityManager.getMyMemoryState(self);
        if (self.importance > android.app.ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND) {
            return "Open Korri on this device to start a game here";
        }
        try {
            JSONObject plan = new JSONObject(planJson);
            String pkg = plan.getString("package");
            String copied = copySaves(pkg, savesJson, true);
            if (copied != null) {
                return copied;
            }
            Intent intent;
            if (plan.has("activity")) {
                intent = new Intent();
                intent.setComponent(new ComponentName(pkg, plan.getString("activity")));
            } else {
                // Updates can rename the launcher activity; resolve it now.
                intent = context.getPackageManager().getLaunchIntentForPackage(pkg);
                if (intent == null) {
                    return "That app has no launcher entry";
                }
            }
            if (plan.has("action")) {
                intent.setAction(plan.getString("action"));
            }
            if ("content".equals(plan.optString("data", null))) {
                if (contentPath.isEmpty()) {
                    return "This launch has no game file";
                }
                Uri uri = FileProvider.getUriForFile(
                        context, context.getPackageName() + ".fileprovider", new File(contentPath));
                intent.setDataAndType(uri, "application/octet-stream");
                // Read-only, for this one file. Plugins cannot ask for more.
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            }
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            context.startActivity(intent);
            watch(pkg);
            return "ok";
        } catch (Exception e) {
            Log.w(TAG, "start failed", e);
            return "The game's app could not start";
        }
    }

    public static String resume(String packageName) {
        Intent intent = context.getPackageManager().getLaunchIntentForPackage(packageName);
        if (intent == null) {
            return "That app is no longer installed";
        }
        // A launcher intent with NEW_TASK brings an existing task forward.
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        context.startActivity(intent);
        watch(packageName);
        return "ok";
    }

    /** End: a Korri-built app quits and its saves come back; any other app keeps running. */
    public static String endSession(String packageName, String savesJson) {
        watched = null;
        dev.korri.savebridge.KorriSaveBridgeClient bridge =
                dev.korri.savebridge.KorriSaveBridgeClient.forPackage(context, packageName);
        String result = "ok";
        if (bridge != null) {
            try {
                bridge.quit();
                // The engine flushes on its normal shutdown path.
                Thread.sleep(2000);
            } catch (Exception e) {
                Log.w(TAG, "quit failed", e);
            }
            String copied = copySaves(packageName, savesJson, false);
            if (copied != null) {
                result = copied;
            }
        }
        showKorri();
        return result;
    }

    /** Copy every declared save folder in (before start) or out (after End). Null on success. */
    private static String copySaves(String packageName, String savesJson, boolean in) {
        dev.korri.savebridge.KorriSaveBridgeClient bridge =
                dev.korri.savebridge.KorriSaveBridgeClient.forPackage(context, packageName);
        if (bridge == null) {
            return null; // Case 4: saves stay with the app.
        }
        try {
            JSONArray saves = new JSONArray(savesJson);
            for (int i = 0; i < saves.length(); i++) {
                JSONObject save = saves.getJSONObject(i);
                File folder = new File(save.getString("path"));
                if (in) {
                    folder.mkdirs();
                    bridge.copyIn(save.getString("key"), folder);
                } else {
                    bridge.copyOut(save.getString("key"), folder);
                }
            }
            return null;
        } catch (Exception e) {
            Log.w(TAG, "save bridge copy failed", e);
            return in ? "Korri could not hand the saves to the game" : "Korri could not take the saves back";
        }
    }

    public static String showKorri() {
        Intent intent = new Intent(context, com.limelight.KorriShellActivity.class);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT);
        context.startActivity(intent);
        return "ok";
    }

    // ---- Installed apps for @korri:android-app discovery. ----

    /** Every launchable app, with Android's game marking. Korri itself is left out. */
    public static String installedAppsJson(Context ctx, java.util.Set<String> added) {
        PackageManager pm = ctx.getPackageManager();
        Intent main = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        List<ResolveInfo> apps = pm.queryIntentActivities(main, 0);
        JSONArray out = new JSONArray();
        java.util.Set<String> seen = new java.util.HashSet<>();
        for (ResolveInfo info : apps) {
            ApplicationInfo app = info.activityInfo.applicationInfo;
            if (app.packageName.equals(ctx.getPackageName()) || !seen.add(app.packageName)) {
                continue;
            }
            boolean game = (android.os.Build.VERSION.SDK_INT >= 26
                    && app.category == ApplicationInfo.CATEGORY_GAME)
                    || (app.flags & ApplicationInfo.FLAG_IS_GAME) != 0;
            try {
                out.put(new JSONObject()
                        .put("packageName", app.packageName)
                        .put("label", String.valueOf(info.loadLabel(pm)))
                        .put("isGame", game)
                        .put("added", added.contains(app.packageName)));
            } catch (org.json.JSONException ignored) {
                // A label that cannot be encoded is skipped.
            }
        }
        return out.toString();
    }

    public static void reportInstalledApps(Context ctx) {
        java.util.Set<String> added = ctx.getSharedPreferences("korri-android-apps", Context.MODE_PRIVATE)
                .getStringSet("added", java.util.Collections.emptySet());
        KorridServer.reportInstalledApps(installedAppsJson(ctx, added));
    }

    // ---- Usage events: noticing that the game left or closed. ----

    private static void watch(String pkg) {
        watched = pkg;
        handler.removeCallbacks(poll);
        handler.postDelayed(poll, USAGE_POLL_MS * 3);
    }

    private static final Runnable poll = new Runnable() {
        private long since = System.currentTimeMillis();

        @Override
        public void run() {
            String pkg = watched;
            if (pkg == null) {
                return;
            }
            UsageStatsManager usage = (UsageStatsManager) context.getSystemService(Context.USAGE_STATS_SERVICE);
            long now = System.currentTimeMillis();
            UsageEvents events = usage == null ? null : usage.queryEvents(since, now);
            since = now;
            if (events != null) {
                UsageEvents.Event event = new UsageEvents.Event();
                while (events.getNextEvent(event)) {
                    if (!pkg.equals(event.getPackageName())) {
                        continue;
                    }
                    // Android shows apps no public "closed" event: a pause
                    // means the person left. End or Korri's own screen ends
                    // the live session.
                    if (event.getEventType() == UsageEvents.Event.ACTIVITY_PAUSED) {
                        KorridServer.appLeft(pkg, false);
                    }
                }
            }
            handler.postDelayed(this, USAGE_POLL_MS);
        }
    };

    /** Usage access is a special app-op the person grants in Settings. */
    public static boolean hasUsageAccess(Context ctx) {
        if (android.os.Build.VERSION.SDK_INT < 29) {
            return false;
        }
        android.app.AppOpsManager ops = (android.app.AppOpsManager) ctx.getSystemService(Context.APP_OPS_SERVICE);
        return ops.unsafeCheckOpNoThrow(android.app.AppOpsManager.OPSTR_GET_USAGE_STATS,
                android.os.Process.myUid(), ctx.getPackageName()) == android.app.AppOpsManager.MODE_ALLOWED;
    }
}
