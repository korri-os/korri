package com.simonwjackson.korri.korrid;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;
import android.util.Log;


import java.io.File;
import java.io.IOException;

/**
 * Keeps korrid alive while Korri is not on screen.
 *
 * Korri's brain is the Rust server behind {@link KorridServer}; this class
 * holds no logic of its own. Its whole job is to be the thing Android keeps
 * running, because Android only extends that courtesy to a declared service.
 *
 * Before this existed, korrid's lifetime was owned by the shell activity and
 * stopped in its onDestroy -- so launching a game, which takes the activity
 * away, also took the brain away. Ownership now sits here: the activity became
 * one client of a running brain rather than the reason it is running.
 */
public final class KorriBrainService extends Service {
    private static final String TAG = "KorriBrain";
    private static final String CHANNEL = "korri-brain";
    private static final int NOTIFICATION_ID = 1;
    private static final String EXTRA_ALLOWED_ORIGIN = "allowedOrigin";
    private static final String EXTRA_LOCAL_STORAGE_ROOT = "localStorageRoot";
    public static final String EXTRA_PRIVATE_STATE_ROOT = "privateStateRoot";

    /**
     * Guards double-starts: the activity and the service both want a running
     * brain, and whoever arrives first should win without the other noticing.
     */
    private static boolean started = false;
    private static int port = 0;

    /**
     * Starts the brain if it is not already up, then asks Android to keep the
     * process around. Synchronous, so callers can rely on the port immediately
     * rather than racing service startup.
     */
    public static synchronized int ensureRunning(
            Context context, String allowedOrigin, String localStorageRoot) {
        String privateStateRoot = privateStateRoot(context);
        startBrainIfNeeded(context, allowedOrigin, localStorageRoot, privateStateRoot);
        Intent intent = launchIntent(context, allowedOrigin, localStorageRoot);
        try {
            if (Build.VERSION.SDK_INT >= 26) {
                context.getApplicationContext().startForegroundService(intent);
            } else {
                context.getApplicationContext().startService(intent);
            }
        } catch (RuntimeException error) {
            stopBrain();
            throw error;
        }
        return port;
    }

    /** Explicit shutdown. Deliberately not called when a screen goes away. */
    public static void shutdown(Context context) {
        if (!context.getApplicationContext()
                .stopService(new Intent(context.getApplicationContext(), KorriBrainService.class))) {
            stopBrain();
        }
    }

    public static String privateStateRoot(Context context) {
        File noBackup = context.getApplicationContext().getNoBackupFilesDir();
        // ContextImpl places no_backup directly inside the app data directory.
        // Android can symlink its user-directory prefix in the app mount namespace.
        // Resolve only that system-owned prefix. Keep app-owned symlinks visible
        // to Rust's strict private-storage validation, including korrid-state itself.
        File appData = noBackup.getParentFile();
        try {
            File userData = appData.getParentFile().getCanonicalFile();
            File resolvedAppData = new File(userData, appData.getName());
            return new File(new File(resolvedAppData, noBackup.getName()), "korrid-state")
                    .getAbsolutePath();
        } catch (IOException error) {
            throw new IllegalStateException("failed to resolve Android private storage prefix", error);
        }
    }

    public static Intent launchIntent(
            Context context, String allowedOrigin, String localStorageRoot) {
        Context application = context.getApplicationContext();
        return new Intent(application, KorriBrainService.class)
                .putExtra(EXTRA_ALLOWED_ORIGIN, allowedOrigin)
                .putExtra(EXTRA_LOCAL_STORAGE_ROOT, localStorageRoot)
                .putExtra(EXTRA_PRIVATE_STATE_ROOT, privateStateRoot(application));
    }

    private static synchronized void startBrainIfNeeded(
            Context context, String allowedOrigin, String localStorageRoot, String privateStateRoot) {
        if (started) {
            return;
        }
        int startedPort = KorridServer.startAndLog(context, allowedOrigin, localStorageRoot, privateStateRoot);
        port = startedPort;
        started = true;
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        startForeground(NOTIFICATION_ID, buildNotification());
        if (!started) {
            if (intent == null) {
                Log.e(TAG, "brain service restarted without launch parameters; stopping");
                return stopWithoutRestart(startId);
            }
            String allowedOrigin = intent.getStringExtra(EXTRA_ALLOWED_ORIGIN);
            String localStorageRoot = intent.getStringExtra(EXTRA_LOCAL_STORAGE_ROOT);
            String privateStateRoot = intent.getStringExtra(EXTRA_PRIVATE_STATE_ROOT);
            if (allowedOrigin == null || localStorageRoot == null || privateStateRoot == null) {
                Log.e(TAG, "brain service launch parameters missing; stopping");
                return stopWithoutRestart(startId);
            }
            try {
                synchronized (KorriBrainService.class) {
                    startBrainIfNeeded(this, allowedOrigin, localStorageRoot, privateStateRoot);
                }
            } catch (RuntimeException error) {
                Log.e(TAG, "brain service failed to start korrid; stopping", error);
                return stopWithoutRestart(startId);
            }
        }
        Log.i(TAG, "brain service ready");
        // If Android kills the process, redeliver the intent so the service
        // can restart the Rust brain with the same portal origin and storage root.
        return START_REDELIVER_INTENT;
    }

    @Override
    public void onDestroy() {
        Log.i(TAG, "brain service stopping, korrid going down with it");
        stopBrain();
        super.onDestroy();
    }

    private static synchronized void stopBrain() {
        try {
            if (started) {
                com.limelight.KorriStreamViewer.shutdown();
                KorridServer.stop();
            }
        } finally {
            started = false;
            port = 0;
        }
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    private int stopWithoutRestart(int startId) {
        removeForegroundState();
        stopSelf(startId);
        return START_NOT_STICKY;
    }

    @SuppressWarnings("deprecation")
    private void removeForegroundState() {
        if (Build.VERSION.SDK_INT >= 24) {
            stopForeground(STOP_FOREGROUND_REMOVE);
        } else {
            stopForeground(true);
        }
    }

    private Notification buildNotification() {
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (Build.VERSION.SDK_INT >= 26) {
            // IMPORTANCE_LOW: required to exist, but it should never make noise.
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL, "Korri", NotificationManager.IMPORTANCE_LOW);
            channel.setDescription("Shown while Korri is ready to play.");
            manager.createNotificationChannel(channel);
        }
        Notification.Builder builder = Build.VERSION.SDK_INT >= 26
                ? new Notification.Builder(this, CHANNEL)
                : new Notification.Builder(this);
        return builder
                .setContentTitle("Korri")
                .setContentText("Ready to play")
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setOngoing(true)
                .build();
    }
}
