package com.limelight;

import android.view.KeyEvent;
import android.view.Surface;
import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;

/** One host-provided Surface. No Activity, stream or game knowledge crosses this bridge. */
public final class KorriSurfaceHost {
    private static Attachment active;

    public static final class Attachment {
        final Surface surface;
        final float density;
        final CountDownLatch released = new CountDownLatch(1);
        final ArrayList<Integer> keys = new ArrayList<>();
        final Runnable failure;
        boolean taken;
        boolean closing;
        boolean resized;
        Attachment(Surface surface, float density, Runnable failure) {
            this.surface = surface;
            this.density = density;
            this.failure = failure;
        }
    }

    public static synchronized Attachment attach(Surface surface, float density, Runnable failure) {
        if (active != null) throw new IllegalStateException("An overlay Surface is already attached");
        active = new Attachment(surface, density, failure);
        return active;
    }

    /** Called inside surfaceDestroyed. Never return while Rust holds the Surface. */
    public static void detach(Attachment attachment) {
        if (attachment == null) return;
        synchronized (KorriSurfaceHost.class) {
            if (active != attachment) return;
            attachment.closing = true;
            attachment.keys.clear();
            if (!attachment.taken) acknowledgeRelease(false);
        }
        boolean interrupted = false;
        for (;;) {
            try { attachment.released.await(); break; }
            catch (InterruptedException error) { interrupted = true; }
        }
        if (interrupted) Thread.currentThread().interrupt();
    }

    public static synchronized void resized(Attachment attachment) {
        if (attachment != null && active == attachment) attachment.resized = true;
    }

    public static synchronized boolean forward(KeyEvent event) {
        if (active == null) return false;
        if (!active.closing && (event.getAction() == KeyEvent.ACTION_UP
                || (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0))) {
            // Guide is host-owned. In an overlay it has the same meaning as Back.
            int code = event.getKeyCode() == KeyEvent.KEYCODE_BUTTON_MODE
                    ? KeyEvent.KEYCODE_BACK : event.getKeyCode();
            active.keys.add(event.getDeviceId());
            active.keys.add(code);
            active.keys.add(event.getAction() == KeyEvent.ACTION_DOWN ? 1 : 0);
        }
        return true;
    }

    public static synchronized Surface takeSurface() {
        if (active == null || active.taken || active.closing) return null;
        active.taken = true;
        return active.surface;
    }
    public static synchronized float density() { return active == null ? 1 : active.density; }
    public static synchronized boolean takeResized() {
        if (active == null) return false;
        boolean resized = active.resized;
        active.resized = false;
        return resized;
    }
    public static synchronized boolean closing() { return active != null && active.closing; }
    public static synchronized void acknowledgeRelease(boolean failed) {
        if (active == null) return;
        Attachment released = active;
        active = null;
        released.released.countDown();
        if (failed) new android.os.Handler(android.os.Looper.getMainLooper()).post(released.failure);
    }
    public static synchronized int[] takeKeys() {
        if (active == null) return new int[0];
        int[] keys = new int[active.keys.size()];
        for (int i = 0; i < keys.length; i++) keys[i] = active.keys.get(i);
        active.keys.clear();
        return keys;
    }

    private KorriSurfaceHost() { }
}
