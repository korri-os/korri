package com.limelight;

import android.view.KeyEvent;
import android.view.InputDevice;
import android.view.MotionEvent;
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
        final ArrayList<Integer> axes = new ArrayList<>();
        final ArrayList<Float> pointers = new ArrayList<>();
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
            attachment.axes.clear();
            attachment.pointers.clear();
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

    /** Android keeps these keys regardless of which gameplay overlay is shown. */
    static boolean isSystemKey(int code) {
        return code == KeyEvent.KEYCODE_VOLUME_UP
                || code == KeyEvent.KEYCODE_VOLUME_DOWN
                || code == KeyEvent.KEYCODE_VOLUME_MUTE
                || code == KeyEvent.KEYCODE_POWER;
    }

    public static synchronized boolean forward(KeyEvent event) {
        if (active == null || isSystemKey(event.getKeyCode())) return false;
        if (!active.closing && (event.getAction() == KeyEvent.ACTION_UP
                || (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0))) {
            // Guide is host-owned. Menu dismisses the overlay even over a question.
            int code = event.getKeyCode() == KeyEvent.KEYCODE_BUTTON_MODE
                    ? KeyEvent.KEYCODE_MENU : event.getKeyCode();
            active.keys.add(event.getDeviceId());
            active.keys.add(code);
            active.keys.add(event.getAction() == KeyEvent.ACTION_DOWN ? 1 : 0);
        }
        return true;
    }

    /** Raw controller samples only; Rust owns threshold, D-pad priority and repeat. */
    public static synchronized boolean forward(MotionEvent event) {
        if (active == null) return false;
        if (active.closing) return true;
        if ((event.getSource() & InputDevice.SOURCE_CLASS_JOYSTICK) == 0
                && !event.isFromSource(InputDevice.SOURCE_DPAD)) return true;
        if (event.getActionMasked() != MotionEvent.ACTION_MOVE
                && event.getActionMasked() != MotionEvent.ACTION_CANCEL) return true;
        active.axes.add(event.getDeviceId());
        for (int axis : new int[] { MotionEvent.AXIS_X, MotionEvent.AXIS_Y,
                MotionEvent.AXIS_HAT_X, MotionEvent.AXIS_HAT_Y }) {
            float value = event.getActionMasked() == MotionEvent.ACTION_CANCEL ? 0 : event.getAxisValue(axis);
            active.axes.add(Float.isNaN(value) || Float.isInfinite(value) ? 0
                    : Math.round(Math.max(-1, Math.min(1, value)) * 1000));
        }
        return true;
    }

    /** Touch in SurfaceView pixels. It never passes through the controller mapper. */
    public static synchronized boolean forwardTouch(MotionEvent event) {
        if (active == null) return false;
        if (active.closing) return true;
        if ((event.getSource() & InputDevice.SOURCE_CLASS_JOYSTICK) != 0
                || event.isFromSource(InputDevice.SOURCE_DPAD)
                || event.isFromSource(InputDevice.SOURCE_GAMEPAD)) return true;
        int action = event.getActionMasked();
        int phase;
        switch (action) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN: phase = 0; break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP: phase = 1; break;
            case MotionEvent.ACTION_MOVE: phase = 2; break;
            case MotionEvent.ACTION_CANCEL:
            case MotionEvent.ACTION_OUTSIDE: phase = 3; break;
            default: return true;
        }
        for (int i = 0; i < event.getPointerCount(); i++) {
            if (phase < 2 && i != event.getActionIndex()) continue;
            float x = event.getX(i), y = event.getY(i);
            if (!Float.isFinite(x) || !Float.isFinite(y)) continue;
            active.pointers.add((float) phase);
            active.pointers.add((float) event.getPointerId(i));
            active.pointers.add(x);
            active.pointers.add(y);
        }
        return true;
    }

    // Four floats per touch sample: phase (down/up/move/cancel = 0/1/2/3), id, X, Y.
    // Rust divides pixels by the hosted Slint window's density, just as NativeActivity does.
    public static synchronized float[] takePointers() {
        if (active == null) return new float[0];
        float[] pointers = new float[active.pointers.size()];
        for (int i = 0; i < pointers.length; i++) pointers[i] = active.pointers.get(i);
        active.pointers.clear();
        return pointers;
    }

    // Five integers per sample: device, X, Y, HAT_X, HAT_Y. Axis units are
    // [-1000,1000], the same measured samples AndroidInput sends to the mapper.
    public static synchronized int[] takeAxes() {
        if (active == null) return new int[0];
        int[] axes = new int[active.axes.size()];
        for (int i = 0; i < axes.length; i++) axes[i] = active.axes.get(i);
        active.axes.clear();
        return axes;
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
