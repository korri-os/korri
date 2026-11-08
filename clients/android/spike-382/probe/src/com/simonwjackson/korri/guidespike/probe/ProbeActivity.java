package com.simonwjackson.korri.guidespike.probe;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.util.Log;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.widget.TextView;

import java.util.ArrayDeque;

/** The "app behind": logs and shows every key and joystick event it receives. */
public class ProbeActivity extends Activity {
    static final String TAG = "KorriGuideProbe";
    private final ArrayDeque<String> lines = new ArrayDeque<>();
    private TextView view;
    private int keys, motions;

    @Override protected void onCreate(Bundle b) {
        super.onCreate(b);
        view = new TextView(this);
        view.setTextSize(18);
        view.setTextColor(Color.BLACK);
        view.setBackgroundColor(0xFF7FD4A0);
        view.setPadding(48, 120, 48, 48);
        setContentView(view);
        add("PROBE APP (behind). Waiting for input.");
    }

    private void add(String s) {
        Log.i(TAG, s);
        lines.addFirst(s);
        while (lines.size() > 18) lines.removeLast();
        StringBuilder sb = new StringBuilder("PROBE  keys=" + keys + "  motions=" + motions + "\n\n");
        for (String l : lines) sb.append(l).append('\n');
        view.setText(sb);
    }

    @Override public boolean dispatchKeyEvent(KeyEvent e) {
        if (e.getKeyCode() == KeyEvent.KEYCODE_BACK) return super.dispatchKeyEvent(e);
        keys++;
        InputDevice d = e.getDevice();
        add("key " + KeyEvent.keyCodeToString(e.getKeyCode()) + " "
                + (e.getAction() == KeyEvent.ACTION_DOWN ? "DOWN" : "UP")
                + " dev=" + (d == null ? "null" : d.getName()));
        return true;
    }

    @Override public boolean dispatchGenericMotionEvent(MotionEvent e) {
        motions++;
        InputDevice d = e.getDevice();
        add(String.format(java.util.Locale.US, "motion x=%.2f y=%.2f hat=%.0f,%.0f dev=%s",
                e.getAxisValue(MotionEvent.AXIS_X), e.getAxisValue(MotionEvent.AXIS_Y),
                e.getAxisValue(MotionEvent.AXIS_HAT_X), e.getAxisValue(MotionEvent.AXIS_HAT_Y),
                d == null ? "null" : d.getName()));
        return true;
    }

    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        add("window focus=" + hasFocus);
    }

    @Override protected void onPause() { super.onPause(); add("onPause"); }
    @Override protected void onResume() { super.onResume(); add("onResume"); }
}
