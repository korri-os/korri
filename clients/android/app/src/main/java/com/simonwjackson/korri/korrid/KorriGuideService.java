package com.simonwjackson.korri.korrid;

import android.accessibilityservice.AccessibilityService;
import android.view.KeyEvent;
import android.view.accessibility.AccessibilityEvent;

/**
 * ADR 0054: catches only the controller's Guide button while another app has
 * focus, and opens Korri. Every other key passes through untouched. The
 * person turns this on in Android's Accessibility settings; without it,
 * games still launch but Guide does nothing over them.
 */
public final class KorriGuideService extends AccessibilityService {
    @Override
    protected boolean onKeyEvent(KeyEvent event) {
        if (event.getKeyCode() != KeyEvent.KEYCODE_BUTTON_MODE) {
            return false;
        }
        // A focused stream keeps the see-through surface inside Game (#376).
        // Native/external apps retain ADR 0054's existing show-Korri behavior.
        if (com.limelight.KorriStreamViewer.forwardGuide(event)) return true;
        if (event.getAction() == KeyEvent.ACTION_UP) {
            // Korri's shell opens its gameplay overlay for the live session.
            // A see-through window over the game is a later step (ADR 0053).
            KorriAppLauncher.showKorri();
        }
        return true;
    }

    @Override
    public void onAccessibilityEvent(AccessibilityEvent event) {}

    @Override
    public void onInterrupt() {}
}
