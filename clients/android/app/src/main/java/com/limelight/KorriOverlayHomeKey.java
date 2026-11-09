package com.limelight;

import android.view.KeyEvent;
import java.util.HashMap;

/** A complete Guide/Back press belongs to one overlay state, even if it closes mid-press. */
final class KorriOverlayHomeKey {
    enum Action { UNHANDLED, CONSUMED, OPEN, CLOSE }
    private final HashMap<Long, Boolean> pressed = new HashMap<>();

    Action handle(KeyEvent event, boolean overlayOpen) {
        int code = event.getKeyCode();
        if (code != KeyEvent.KEYCODE_BUTTON_MODE && code != KeyEvent.KEYCODE_BACK) return Action.UNHANDLED;
        long key = ((long) event.getDeviceId() << 32) | (code & 0xffffffffL);
        if (event.getAction() == KeyEvent.ACTION_DOWN) {
            if (event.getRepeatCount() == 0 && !pressed.containsKey(key)) pressed.put(key, overlayOpen);
            return Action.CONSUMED;
        }
        if (event.getAction() == KeyEvent.ACTION_UP) {
            Boolean wasOpen = pressed.remove(key);
            if (wasOpen == null || event.isCanceled()) return Action.CONSUMED;
            return wasOpen ? Action.CLOSE : Action.OPEN;
        }
        return Action.CONSUMED;
    }
}
