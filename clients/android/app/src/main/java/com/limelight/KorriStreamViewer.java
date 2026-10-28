package com.limelight;

import android.app.ActivityManager;
import android.content.Context;
import android.content.Intent;
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

/** Native platform effects only. Rust selects, authorizes and controls the far launch. */
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

    public static String start(String launchId, String address) throws Exception {
        requireWorker();
        KorriStreamViewer host = host();
        synchronized (host.effects) {
            Session prior = host.session;
            if (prior != null && prior.state.phase() != Phase.STOPPED) throw failure();
            Session current = new Session(launchId);
            host.session = current;
            try {
                current.intent = host.prepare(current, address);
                host.launch(current);
                if (!current.state.await(Phase.RUNNING, START_TIMEOUT_MS)) throw failure();
                return "Running";
            } catch (Exception error) {
                // A failed start must release its own viewer before Rust performs
                // compensation on the far exact launch. Never target another ID.
                // Service teardown can run on the main thread while it joins
                // Rust. Do not make that thread wait on its own Activity callbacks.
                // Retirement is failure, never a confirmed stopped viewer.
                if (!current.retired) host.stopAndAwait(current);
                throw failure();
            }
        }
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
            if (current.state.phase() == Phase.RUNNING) return "Running";
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

    private Intent prepare(Session current, String address) throws Exception {
        ComputerDetails.AddressTuple tuple = KorriMoonlightAddressParser.parse(address);
        if (tuple == null) throw failure();
        String uniqueId = new IdentityManager(context).getUniqueId();
        NvHTTP discovery = new NvHTTP(tuple, 0, uniqueId, null, PlatformBinding.getCryptoProvider(context));
        ComputerDetails computer = discovery.getComputerDetails(true);
        String clientPem = new String(PlatformBinding.getCryptoProvider(context)
                .getPemEncodedClientCertificate(), StandardCharsets.US_ASCII);
        JSONObject provision = new JSONObject(KorridServer.provisionMoonlightCertificate(computer.uuid, clientPem));
        if (!"Provisioned".equals(provision.optString("_tag"))) throw failure();
        X509Certificate certificate = KorriMoonlightProvisioning.parseSingleCertificate(
                provision.getString("serverCertificate"));
        NvHTTP trusted = new NvHTTP(tuple, computer.httpsPort, uniqueId, certificate,
                PlatformBinding.getCryptoProvider(context));
        if (trusted.getPairState() != HostCertificateState.ACCEPTED) throw failure();
        NvApp selected = streamApp(trusted.getAppList());
        // These extras are the retained ServerHelper/Game decoder contract.
        return new Intent(context, Game.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
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

    static void leave(Game game, long lease) {
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
                            android.widget.Toast.makeText(game, R.string.korri_stream_leave_failed,
                                    android.widget.Toast.LENGTH_LONG).show();
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
        if (current != null) current.state.connected(lease);
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
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current != null) current.state.closed(lease);
    }
    static void destroyed(Game game, long lease) {
        KorriStreamViewer host = instance;
        Session current = host == null ? null : host.source(game);
        if (current == null) return;
        synchronized (current.activities) { current.activities.remove(lease); }
        current.state.destroyed(lease, game.isChangingConfigurations());
    }
}
