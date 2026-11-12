package com.limelight;

import android.app.ActivityManager;
import android.content.Context;
import android.content.Intent;
import android.content.res.Configuration;
import android.graphics.Point;
import android.hardware.display.DisplayManager;
import android.os.Build;
import android.view.Display;

import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.korri.moonlight.KorriMoonlightActionExecutor.Effect;
import android.os.Handler;
import android.os.Looper;

import com.limelight.binding.PlatformBinding;
import com.limelight.computers.IdentityManager;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCertificateState;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.simonwjackson.korri.korrid.KorridServer;

import org.json.JSONObject;

import java.nio.charset.StandardCharsets;
import java.security.cert.X509Certificate;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

import static com.limelight.KorriStreamViewerState.Phase;

/** Native platform effects only. Rust selects, authorizes and controls the streamed launch. */
public final class KorriStreamViewer {
    public static final String EXTRA_GENERATION = "KorriViewerGeneration";
    private static final long START_TIMEOUT_MS = 45_000;
    private static final long CONTROL_TIMEOUT_MS = 15_000;
    private static final AtomicLong GENERATIONS = new AtomicLong();
    private static volatile KorriStreamViewer instance;

    private final Context context;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Object effects = new Object();
    private final ExecutorService input = Executors.newSingleThreadExecutor();
    private volatile Session session;

    private static final class Session {
        final KorriStreamViewerState state;
        final KorriStreamControlState controls = new KorriStreamControlState();
        final Map<Long, Game> activities = new HashMap<>();
        Intent intent;
        final AtomicBoolean leavePending = new AtomicBoolean();
        volatile boolean retired;
        Session(String launchId) {
            state = new KorriStreamViewerState(launchId, GENERATIONS.incrementAndGet());
        }
    }

    private KorriStreamViewer(Context context) {
        this.context = context.getApplicationContext();
    }

    public static synchronized void initialize(Context context) {
        if (instance == null) instance = new KorriStreamViewer(context);
    }

    /** ADR 0054's global Guide service must preserve Game's in-place overlay path. */
    public static boolean forwardGuide(android.view.KeyEvent event) {
        if (event.getKeyCode() != android.view.KeyEvent.KEYCODE_BUTTON_MODE) return false;
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.session;
        if (current == null) return false;
        synchronized (current.state) {
            long lease = current.state.activityLease();
            if (host.session != current || current.retired || !current.state.canPresent(lease)
                    || current.state.phase() != Phase.RUNNING) return false;
            Game game;
            synchronized (current.activities) { game = current.activities.get(lease); }
            return game != null && game.dispatchKeyEvent(event);
        }
    }

    public static void shutdown() {
        KorriStreamViewer host = instance;
        if (host == null) return;
        Session current = host.session;
        if (current == null) return;
        current.retired = true;
        current.state.stop();
        host.finishActivities(current);
    }

    private static KorriStreamViewer host() {
        KorriStreamViewer host = instance;
        if (host == null) throw failure();
        return host;
    }

    private static IllegalStateException failure() {
        // Platform exception text can contain addresses or certificate material.
        return new IllegalStateException("Exact Android stream viewer effect failed");
    }

    private static void requireWorker() {
        if (Looper.myLooper() == Looper.getMainLooper()) throw failure();
    }

    /**
     * Temporary rule until stream size becomes a cascading setting: a Korri
     * viewer offers the screen's own resolution to the holder's korrid. Landscape pair (long side first), the form of
     * {@code prefConfig.width/height}; Game inverts it in portrait.
     */
    static int[] screenStreamSize(Context context) {
        DisplayManager displays = (DisplayManager) context.getSystemService(Context.DISPLAY_SERVICE);
        Display display = displays == null ? null : displays.getDisplay(Display.DEFAULT_DISPLAY);
        if (display == null) return null;
        int width, height;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            width = display.getMode().getPhysicalWidth();
            height = display.getMode().getPhysicalHeight();
        } else {
            Point size = new Point();
            display.getRealSize(size);
            width = size.x;
            height = size.y;
        }
        if (width <= 0 || height <= 0) return null;
        return new int[] {Math.max(width, height), Math.min(width, height)};
    }

    /** Screen geometry defaults to landscape; Portrait is a session control. */
    public static String viewerDisplay() {
        KorriStreamViewer host = instance;
        if (host == null) return "";
        int[] size = screenStreamSize(host.context);
        if (size == null) return "";
        return size[0] + "x" + size[1];
    }

    public static String start(String launchId, String address, int width, int height) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        synchronized (host.effects) {
            Session prior = host.session;
            if (prior != null && prior.state.phase() != Phase.STOPPED) throw failure();
            Session current = new Session(launchId);
            host.session = current;
            try {
                current.intent = host.prepare(current, address, width, height);
                host.launch(current);
                if (!current.state.await(Phase.RUNNING, START_TIMEOUT_MS)) throw failure();
                return "Running";
            } catch (Exception error) {
                LimeLog.warning("Exact Android viewer start failed: phase=" + current.state.phase()
                        + " error=" + error.getClass().getSimpleName());
                // A failed start must release its own viewer before Rust performs
                // compensation on the exact streamed launch. Never target another ID.
                // Service teardown can run on the main thread while it joins
                // Rust. Do not make that thread wait on its own Activity callbacks.
                // Retirement is failure, never a confirmed stopped viewer.
                if (!current.retired) host.stopAndAwait(current);
                throw failure();
            }
        }
    }

    /** Exact worker-to-UI read/apply seam; no viewer state is published by Java. */
    private static String withGame(String launchId, String control, String value) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        Session current = host.requireExact(launchId);
        java.util.concurrent.FutureTask<String> task = new java.util.concurrent.FutureTask<>(() -> {
            synchronized (current.state) {
                long lease = current.state.activityLease();
                Phase phase = current.state.phase();
                if (host.session != current || current.retired || !current.state.canPresent(lease)
                        || (phase != Phase.RUNNING && phase != Phase.BACKGROUNDED)) throw failure();
                Game game;
                synchronized (current.activities) { game = current.activities.get(lease); }
                if (game == null) throw failure();
                String prefix = KorriViewerControls.prefix(launchId, current.state.generation, lease);
                if (control == null) return KorriViewerControls.snapshot(game, prefix);
                if (!KorriViewerControls.apply(game, launchId, prefix, control, value, current.controls)) {
                    // Rust resumed first for stream-input commands. Restore the overlay
                    // on a refused effect so its error is visible, without retargeting a lease.
                    if (control.startsWith(prefix)) {
                        Effect effect = Effect.fromWire(control.substring(prefix.length()));
                        if (effect != null && KorriViewerControls.dismiss(effect)) game.showGameMenu();
                    }
                    throw failure();
                }
                return "Applied";
            }
        });
        host.main.post(task);
        try { return task.get(5, java.util.concurrent.TimeUnit.SECONDS); }
        finally { task.cancel(false); }
    }

    public static String notice(String launchId, String message) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        Session current = host.requireExact(launchId);
        host.main.post(() -> {
            if (host.session == current && !current.retired) {
                android.widget.Toast.makeText(host.context, message, android.widget.Toast.LENGTH_SHORT).show();
            }
        });
        return "Queued";
    }

    public static String resize(String launchId, String connection, int width, int height) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        Session current = host.requireExact(launchId);
        final Game game;
        final long lease;
        synchronized (current.state) {
            lease = current.state.activityLease();
            String prefix = KorriViewerControls.prefix(launchId, current.state.generation, lease);
            if (!KorriViewerControls.connection(prefix).equals(connection) || !current.state.canPresent(lease)) throw failure();
            synchronized (current.activities) { game = current.activities.get(lease); }
        }
        if (game == null) throw failure();
        com.limelight.korri.moonlight.KorriSunshineRuntimeSettings.Authorization authority =
            new com.limelight.korri.moonlight.KorriSunshineRuntimeSettings.Authorization() {
                public boolean isCurrent() {
                    synchronized (current.state) { return host.session == current && !current.retired && current.state.canPresent(lease); }
                }
                public <T> T commit(java.util.function.Supplier<T> action, T stale) {
                    synchronized (current.state) { return isCurrent() ? action.get() : stale; }
                }
            };
        if (!game.resizeLiveStream(width, height, authority)) throw failure();
        return "Applied";
    }

    public static String snapshot(String launchId) throws Exception {
        KorriStreamViewer host = host();
        Session current = host.requireExact(launchId);
        synchronized (current.state) {
            long lease = current.state.activityLease();
            if (!current.state.connectedTransport() || lease == 0) {
                return new JSONObject().put("controls", new org.json.JSONArray())
                        .put("connected", current.state.connectedTransport()).toString();
            }
        }
        return withGame(launchId, null, null);
    }

    public static String apply(String launchId, String control, String value) throws Exception {
        return withGame(launchId, control, value);
    }

    public static String status(String launchId) {
        Session current = host().exact(launchId);
        if (current == null) return "Stopped";
        switch (current.state.phase()) {
            case RUNNING: return "Running";
            case BACKGROUNDED: return "Backgrounded";
            case STOPPED: return "Stopped";
            default: throw failure();
        }
    }

    public static String background(String launchId) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        synchronized (host.effects) {
            Session current = host.requireExact(launchId);
            if (current.state.phase() == Phase.STOPPED) throw failure();
            current.state.background();
            host.main.post(() -> {
                if (host.session != current || current.state.phase() != Phase.BACKGROUNDING) return;
                try {
                    host.context.startActivity(new Intent(host.context, KorriShellActivity.class)
                            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT));
                    // PiP or platform preferences must not retain a decoder after
                    // the exact background effect has requested full release.
                    host.finishActivities(current);
                } catch (RuntimeException error) {
                    // Keep the unconfirmed transition, rather than report success.
                }
            });
            if (!current.state.await(Phase.BACKGROUNDED, CONTROL_TIMEOUT_MS)) throw failure();
            return "Backgrounded";
        }
    }

    public static String resume(String launchId) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        synchronized (host.effects) {
            Session current = host.requireExact(launchId);
            if (current.state.phase() == Phase.RUNNING) {
                java.util.concurrent.CountDownLatch released = new java.util.concurrent.CountDownLatch(1);
                host.main.post(() -> {
                    try {
                        synchronized (current.activities) {
                            Game activity = current.activities.get(current.state.activityLease());
                            if (host.session == current && activity != null) activity.hideSurfaceOverlay();
                        }
                    } finally { released.countDown(); }
                });
                if (!released.await(CONTROL_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS)) throw failure();
                return "Running";
            }
            if (current.state.phase() != Phase.BACKGROUNDED) throw failure();
            Game activity;
            synchronized (current.activities) {
                activity = current.activities.get(current.state.activityLease());
            }
            if (activity == null) {
                current.state.resume();
                host.launch(current);
            } else {
                // Raise the held Activity's exact task, not the most recent Game.
                host.main.post(() -> {
                    if (host.session != current) return;
                    try {
                        ActivityManager manager = (ActivityManager) host.context.getSystemService(Context.ACTIVITY_SERVICE);
                        for (ActivityManager.AppTask task : manager.getAppTasks()) {
                            if (task.getTaskInfo().id == activity.getTaskId()) {
                                task.moveToFront();
                                return;
                            }
                        }
                    } catch (RuntimeException error) {
                        // The worker reports an unconfirmed focus effect, not a
                        // main-thread crash or a successful resume.
                    }
                });
            }
            if (!current.state.await(Phase.RUNNING, START_TIMEOUT_MS)) throw failure();
            return "Running";
        }
    }

    public static String stop(String launchId) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        synchronized (host.effects) {
            Session current = host.exact(launchId);
            if (current != null) host.stopAndAwait(current);
            return "Stopped";
        }
    }

    private Session exact(String launchId) {
        if (launchId == null || !launchId.matches("[0-9a-f]{32}")) throw failure();
        Session current = session;
        return current != null && current.state.launchId.equals(launchId) ? current : null;
    }

    private Session requireExact(String launchId) {
        Session current = exact(launchId);
        if (current == null || current.retired) throw failure();
        return current;
    }

    private void stopAndAwait(Session current) throws InterruptedException {
        current.state.stop();
        finishActivities(current);
        if (!current.state.await(Phase.STOPPED, CONTROL_TIMEOUT_MS)) throw failure();
    }

    private void finishActivities(Session current) {
        main.post(() -> {
            ArrayList<Game> activities;
            synchronized (current.activities) {
                activities = new ArrayList<>(current.activities.values());
            }
            for (Game activity : activities) activity.finish();
        });
    }

    private void launch(Session current) {
        main.post(() -> {
            if (session != current || current.state.phase() != Phase.STARTING) return;
            try {
                context.startActivity(new Intent(current.intent));
            } catch (RuntimeException error) {
                current.state.failedToAttach();
            }
        });
    }

    private Intent prepare(Session current, String address, int width, int height) throws Exception {
        // Log only fixed stage names and exception classes. Exception messages,
        // addresses, IDs and certificate material must not enter diagnostics.
        String stage = "address";
        try {
            ComputerDetails.AddressTuple tuple = KorriMoonlightAddressParser.parse(address);
            if (tuple == null) throw failure();
            String uniqueId = new IdentityManager(context).getUniqueId();
            NvHTTP discovery = new NvHTTP(tuple, 0, uniqueId, null, PlatformBinding.getCryptoProvider(context));
            stage = "http-serverinfo";
            ComputerDetails computer = discovery.getComputerDetails(true);
            String clientPem = new String(PlatformBinding.getCryptoProvider(context)
                    .getPemEncodedClientCertificate(), StandardCharsets.US_ASCII);
            stage = "certificate-provision";
            JSONObject provision = new JSONObject(KorridServer.provisionMoonlightCertificate(computer.uuid, clientPem));
            if (!"Provisioned".equals(provision.optString("_tag"))) throw failure();
            stage = "certificate-parse";
            X509Certificate certificate = KorriMoonlightProvisioning.parseSingleCertificate(
                    provision.getString("serverCertificate"));
            NvHTTP trusted = new NvHTTP(tuple, computer.httpsPort, uniqueId, certificate,
                    PlatformBinding.getCryptoProvider(context));
            stage = "https-serverinfo-pair-state";
            if (trusted.getPairState() != HostCertificateState.ACCEPTED) throw failure();
            stage = "https-applist";
            List<NvApp> apps = trusted.getAppList();
            stage = "select-korri-stream";
            NvApp selected = streamApp(apps);
            stage = "game-intent";
            // These extras are the retained ServerHelper/Game decoder contract.
            Intent intent = new Intent(context, Game.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    .putExtra(Game.EXTRA_HOST, tuple.address)
                    .putExtra(Game.EXTRA_PORT, tuple.port)
                    .putExtra(Game.EXTRA_HTTPS_PORT, computer.httpsPort)
                    .putExtra(Game.EXTRA_APP_NAME, selected.getAppName())
                    .putExtra(Game.EXTRA_APP_UUID, selected.getAppUUID())
                    .putExtra(Game.EXTRA_APP_ID, selected.getAppId())
                    .putExtra(Game.EXTRA_APP_HDR, selected.isHdrSupported())
                    .putExtra(Game.EXTRA_UNIQUEID, uniqueId)
                    .putExtra(Game.EXTRA_PC_UUID, computer.uuid)
                    .putExtra(Game.EXTRA_PC_NAME, computer.name)
                    .putExtra(Game.EXTRA_SERVER_CERT, certificate.getEncoded())
                    .putExtra(Game.EXTRA_KORRI_LAUNCH_ID, current.state.launchId)
                    .putExtra(EXTRA_GENERATION, current.state.generation);
            putStreamSize(intent, context, width, height);
            return intent;
        } catch (Exception error) {
            LimeLog.warning("Exact Android viewer prepare failed: stage=" + stage
                    + " error=" + error.getClass().getSimpleName());
            throw error;
        }
    }

    /** The streamed launch fixes the stream size; Game applies portrait inversion. */
    static void putStreamSize(Intent intent, Context context, int width, int height) {
        int[] size = width > 0 && height > 0
                ? new int[] {width, height}
                : screenStreamSize(context);
        if (size != null) {
            intent.putExtra(Game.EXTRA_WIDTH, size[0]).putExtra(Game.EXTRA_HEIGHT, size[1]);
        }
    }

    static NvApp streamApp(List<NvApp> apps) {
        NvApp selected = null;
        // Same producer as Linux's stream_args: one stable Sunshine transport app.
        for (NvApp app : apps) {
            if (!"Korri Stream".equals(app.getAppName())) continue;
            if (selected != null || app.getAppId() <= 0) throw failure();
            selected = app;
        }
        if (selected == null) throw failure();
        return selected;
    }

    private Session source(Game game) {
        Session current = session;
        Intent intent = game.getIntent();
        if (current == null || current.state.generation != intent.getLongExtra(EXTRA_GENERATION, 0)
                || !current.state.launchId.equals(intent.getStringExtra(Game.EXTRA_KORRI_LAUNCH_ID))) return null;
        return current;
    }

    static long attach(Game game) {
        KorriStreamViewer host = instance;
        if (host == null) return 0;
        Session current = host.source(game);
        if (current == null) return 0;
        long lease = current.state.attach(current.state.launchId, current.state.generation);
        if (lease != 0) synchronized (current.activities) { current.activities.put(lease, game); }
        return lease;
    }

    static void leave(Game game, long lease, Runnable failure) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current == null || !current.state.canPresent(lease)
                || !current.leavePending.compareAndSet(false, true)) return;
        host.input.execute(() -> {
            try {
                if (!current.state.canPresent(lease) || host.session != current) return;
                if (!KorridServer.leaveViewer(current.state.launchId)) {
                    host.main.post(() -> {
                        if (host.session == current && current.state.canPresent(lease)) {
                            failure.run();
                        }
                    });
                }
            } finally {
                current.leavePending.set(false);
            }
        });
    }

    static boolean canPresent(Game game, long lease) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        return current != null && current.state.canPresent(lease);
    }

    static boolean opening(Game game, long lease) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        return current != null && current.state.opening(lease);
    }
    static void connected(Game game, long lease) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current != null) synchronized (current.state) {
            if (host.session != current || !current.state.canPresent(lease)) return;
            KorriViewerControls.restore(game, current.controls);
            game.restoreViewerZoomPan(current.controls);
            current.state.connected(lease);
            game.startLiveResolution();
        }
    }
    static void focus(Game game, long lease, boolean focused) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current != null) current.state.focus(lease, focused);
    }
    static void failed(Game game, long lease) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current != null) current.state.failed(lease);
    }
    static void closed(Game game, long lease) {
        game.stopLiveResolution();
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current != null) current.state.closed(lease);
    }
    static void destroyed(Game game, long lease) {
        game.stopLiveResolution();
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current == null) return;
        synchronized (current.state) {
            if (current.state.isCurrent(lease)) game.captureViewerZoomPan(current.controls);
            synchronized (current.activities) { current.activities.remove(lease); }
            current.state.destroyed(lease, game.isChangingConfigurations());
        }
    }
}
