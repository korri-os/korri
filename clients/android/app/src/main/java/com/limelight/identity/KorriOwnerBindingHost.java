package com.limelight.identity;

import android.content.Context;
import android.content.Intent;
import android.os.Handler;
import android.os.Looper;

/** Executes korrid's owner-signing effects on the Android UI thread. The shell
 * never exposes this adapter to JavaScript and never creates launch authority. */
public final class KorriOwnerBindingHost implements AutoCloseable {
    public interface NativeRequests {
        long update(long generation, String publicSnapshot);
        String take(long generation);
        void detach(long generation);
    }

    private static final long POLL_MS = 250;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final KorriOwnerBindingController controller;
    private final NativeRequests requests;
    private final long generation;
    private boolean closed;
    private final Runnable poll = this::consumeRequest;

    public KorriOwnerBindingHost(
            Context context,
            KorriOwnerBindingController.NativeIdentity identity,
            NativeRequests requests,
            Nip55PersonSigner.IntentLauncher launcher) {
        this.requests = requests;
        controller = new KorriOwnerBindingController(identity, this::publish);
        controller.attachSigner(new Nip55PersonSigner(context, launcher, controller));
        long attached;
        try {
            attached = requests.update(0, controller.snapshotJson());
            if (attached <= 0) throw new IllegalStateException("Native signer did not attach");
        } catch (RuntimeException error) {
            closed = true;
            controller.close();
            throw error;
        }
        generation = attached;
        handler.post(poll);
    }

    private void publish() {
        if (!closed && generation > 0) requests.update(generation, controller.snapshotJson());
    }

    private void consumeRequest() {
        if (closed) return;
        try {
            String template = requests.take(generation);
            if (template != null) {
                controller.startOwnerBinding(template);
                publish();
            }
            handler.postDelayed(poll, POLL_MS);
        } catch (RuntimeException error) {
            // The service can be stopped independently of this activity.
            close();
        }
    }

    public void refresh() { publish(); }

    public void onActivityResult(int resultCode, Intent data) {
        if (!closed) controller.onActivityResult(resultCode, data);
    }

    @Override public void close() {
        if (closed) return;
        closed = true;
        handler.removeCallbacks(poll);
        controller.close();
        try { requests.detach(generation); }
        catch (RuntimeException ignored) { /* The brain has already stopped. */ }
    }
}
