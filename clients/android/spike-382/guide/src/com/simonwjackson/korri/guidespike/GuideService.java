package com.simonwjackson.korri.guidespike;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.AccessibilityServiceInfo;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.graphics.Color;
import android.graphics.PixelFormat;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;
import android.view.Gravity;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewTreeObserver;
import android.view.WindowManager;
import android.view.accessibility.AccessibilityEvent;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;

/**
 * Spike #382. Filters key events system-wide; Guide (KEYCODE_BUTTON_MODE)
 * toggles an overlay window over whatever app is in front. While the overlay
 * is up, gamepad keys are consumed here so the app behind never sees them.
 *
 * adb control (all optional):
 *   am broadcast -p com.simonwjackson.korri.guidespike -a com.simonwjackson.korri.guidespike.CONTROL \
 *     --es type acc|app --ez focus true|false --es cmd show|hide|toggle
 */
public class GuideService extends AccessibilityService {
    static final String TAG = "KorriGuideSpike";
    static final String ACTION = "com.simonwjackson.korri.guidespike.CONTROL";

    private final Handler main = new Handler(Looper.getMainLooper());
    private WindowManager wm;
    private OverlayRoot overlay;
    private TextView clock, status, lastKey;
    private String windowType = "acc"; // acc = TYPE_ACCESSIBILITY_OVERLAY, app = TYPE_APPLICATION_OVERLAY
    private boolean focusable = true;
    private CharSequence foreground = "?";
    private int held;

    private final BroadcastReceiver control = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent i) {
            if (i.hasExtra("type")) windowType = i.getStringExtra("type");
            if (i.hasExtra("focus")) focusable = i.getBooleanExtra("focus", true);
            String cmd = i.getStringExtra("cmd");
            Log.i(TAG, "control type=" + windowType + " focus=" + focusable + " cmd=" + cmd);
            if ("show".equals(cmd) && overlay == null) show(SystemClock.uptimeMillis(), "broadcast");
            else if ("hide".equals(cmd)) hide("broadcast");
            else if ("toggle".equals(cmd)) toggle(SystemClock.uptimeMillis(), "broadcast");
        }
    };

    @Override protected void onServiceConnected() {
        wm = (WindowManager) getSystemService(WINDOW_SERVICE);
        AccessibilityServiceInfo info = getServiceInfo();
        Log.i(TAG, "connected sdk=" + Build.VERSION.SDK_INT + " model=" + Build.MODEL
                + " flags=0x" + Integer.toHexString(info.flags)
                + " filterKeys=" + ((info.flags & AccessibilityServiceInfo.FLAG_REQUEST_FILTER_KEY_EVENTS) != 0)
                + " capabilities=0x" + Integer.toHexString(info.getCapabilities()));
        IntentFilter f = new IntentFilter(ACTION);
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(control, f, Context.RECEIVER_EXPORTED);
        else registerReceiver(control, f);
    }

    @Override public void onAccessibilityEvent(AccessibilityEvent e) {
        if (e.getEventType() == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED && e.getPackageName() != null
                && !getPackageName().contentEquals(e.getPackageName())) {
            foreground = e.getPackageName();
            Log.i(TAG, "foreground pkg=" + foreground + " class=" + e.getClassName());
        }
    }

    @Override public void onInterrupt() {}

    @Override public boolean onUnbind(Intent intent) {
        hide("unbind");
        try { unregisterReceiver(control); } catch (RuntimeException ignored) {}
        return super.onUnbind(intent);
    }

    static boolean isGamepad(KeyEvent e) {
        int s = e.getSource();
        InputDevice d = e.getDevice();
        if (d != null) s |= d.getSources();
        return KeyEvent.isGamepadButton(e.getKeyCode())
                || (s & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (s & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
    }

    static String describe(KeyEvent e) {
        InputDevice d = e.getDevice();
        return KeyEvent.keyCodeToString(e.getKeyCode())
                + " action=" + (e.getAction() == KeyEvent.ACTION_DOWN ? "DOWN" : "UP")
                + " repeat=" + e.getRepeatCount()
                + " device=\"" + (d == null ? "null" : d.getName()) + "\""
                + " deviceId=" + e.getDeviceId()
                + " eventSource=0x" + Integer.toHexString(e.getSource())
                + " deviceSources=0x" + (d == null ? "-" : Integer.toHexString(d.getSources()))
                + " virtual=" + (d != null && d.isVirtual())
                + " flags=0x" + Integer.toHexString(e.getFlags());
    }

    @Override protected boolean onKeyEvent(KeyEvent e) {
        boolean pad = isGamepad(e);
        long lag = SystemClock.uptimeMillis() - e.getEventTime();
        Log.i(TAG, "key " + describe(e) + " gamepad=" + pad + " overlay=" + (overlay != null)
                + " fg=" + foreground + " filterLagMs=" + lag);
        if (e.getKeyCode() == KeyEvent.KEYCODE_BUTTON_MODE) {
            if (e.getAction() == KeyEvent.ACTION_DOWN && e.getRepeatCount() == 0) {
                toggle(e.getEventTime(), "guide");
            }
            Log.i(TAG, "consumed GUIDE " + (e.getAction() == KeyEvent.ACTION_DOWN ? "DOWN" : "UP"));
            return true;
        }
        if (overlay != null && pad) {
            held++;
            Log.i(TAG, "HELD from app behind (" + held + "): " + KeyEvent.keyCodeToString(e.getKeyCode()));
            if (lastKey != null && e.getAction() == KeyEvent.ACTION_DOWN) {
                lastKey.setText("held " + held + ": " + KeyEvent.keyCodeToString(e.getKeyCode()));
            }
            return true;
        }
        return false;
    }

    private void toggle(long eventTime, String why) {
        if (overlay == null) show(eventTime, why); else hide(why);
    }

    private void show(final long eventTime, String why) {
        final OverlayRoot root = new OverlayRoot(this);
        root.setBackgroundColor(0x55000000); // light scrim; the app behind stays visible

        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setGravity(Gravity.CENTER);
        panel.setPadding(64, 48, 64, 48);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0xB01B1F24);
        bg.setCornerRadius(36);
        bg.setStroke(4, 0xFFE8A33D);
        panel.setBackground(bg);

        TextView title = text("KORRI OVERLAY", 40, 0xFFE8A33D);
        clock = text("", 28, Color.WHITE);
        status = text("", 16, 0xFFCCCCCC);
        lastKey = text("held 0", 20, Color.WHITE);
        TextView hint = text("Guide or tap to close", 14, 0xFF999999);
        panel.addView(title);
        panel.addView(clock);
        panel.addView(status);
        panel.addView(lastKey);
        panel.addView(hint);

        FrameLayout.LayoutParams plp = new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER);
        root.addView(panel, plp);
        root.setOnClickListener(v -> hide("tap"));

        int type = "app".equals(windowType)
                ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
                : WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY;
        int flags = WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN;
        if (!focusable) flags |= WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE;
        WindowManager.LayoutParams lp = new WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
                type, flags, PixelFormat.TRANSLUCENT);
        if (Build.VERSION.SDK_INT >= 30) {
            lp.layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS;
        }
        lp.setTitle("KorriGuideSpikeOverlay");
        status.setText("type=" + ("app".equals(windowType) ? "TYPE_APPLICATION_OVERLAY" : "TYPE_ACCESSIBILITY_OVERLAY")
                + " focusable=" + focusable + "\nover " + foreground);

        root.getViewTreeObserver().addOnDrawListener(new ViewTreeObserver.OnDrawListener() {
            boolean first = true;
            @Override public void onDraw() {
                if (!first) return;
                first = false;
                Log.i(TAG, "overlay first draw " + (SystemClock.uptimeMillis() - eventTime) + " ms after the event");
            }
        });
        try {
            wm.addView(root, lp);
        } catch (RuntimeException ex) {
            Log.w(TAG, "addView FAILED type=" + windowType + ": " + ex);
            return;
        }
        overlay = root;
        held = 0;
        tick();
        Log.i(TAG, "overlay SHOWN by " + why + " type=" + windowType + " focusable=" + focusable + " over " + foreground);
    }

    private void tick() {
        if (overlay == null) return;
        clock.setText(new SimpleDateFormat("HH:mm:ss", Locale.US).format(new Date()));
        main.postDelayed(this::tick, 1000);
    }

    private void hide(String why) {
        if (overlay == null) return;
        try { wm.removeViewImmediate(overlay); } catch (RuntimeException ignored) {}
        overlay = null;
        main.removeCallbacksAndMessages(null);
        Log.i(TAG, "overlay HIDDEN by " + why + " held=" + held);
    }

    private TextView text(String s, float sp, int color) {
        TextView t = new TextView(this);
        t.setText(s);
        t.setTextSize(sp);
        t.setTextColor(color);
        t.setGravity(Gravity.CENTER);
        return t;
    }

    /** Logs what reaches the overlay window itself (only when it has focus). */
    final class OverlayRoot extends FrameLayout {
        OverlayRoot(Context c) { super(c); setFocusable(true); setFocusableInTouchMode(true); }

        @Override public boolean dispatchKeyEvent(KeyEvent e) {
            Log.i(TAG, "overlay window got key " + describe(e));
            if (e.getKeyCode() == KeyEvent.KEYCODE_BACK && e.getAction() == KeyEvent.ACTION_UP) hide("back");
            return true;
        }

        @Override public boolean dispatchGenericMotionEvent(MotionEvent e) {
            InputDevice d = e.getDevice();
            Log.i(TAG, "overlay window got motion source=0x" + Integer.toHexString(e.getSource())
                    + " device=\"" + (d == null ? "null" : d.getName()) + "\""
                    + " x=" + e.getAxisValue(MotionEvent.AXIS_X) + " y=" + e.getAxisValue(MotionEvent.AXIS_Y)
                    + " hatX=" + e.getAxisValue(MotionEvent.AXIS_HAT_X) + " hatY=" + e.getAxisValue(MotionEvent.AXIS_HAT_Y));
            return true;
        }

        @Override public void onWindowFocusChanged(boolean hasFocus) {
            super.onWindowFocusChanged(hasFocus);
            Log.i(TAG, "overlay window focus=" + hasFocus);
        }
    }
}
