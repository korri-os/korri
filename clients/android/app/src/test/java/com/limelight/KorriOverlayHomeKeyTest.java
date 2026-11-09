package com.limelight;

import android.view.KeyEvent;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import static com.limelight.KorriOverlayHomeKey.Action.*;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorriOverlayHomeKeyTest {
    @Test public void guideAndBackToggleOnceOnlyAfterTheCompletePress() {
        for (int code : new int[] { KeyEvent.KEYCODE_BUTTON_MODE, KeyEvent.KEYCODE_BACK }) {
            KorriOverlayHomeKey keys = new KorriOverlayHomeKey();
            for (int cycle = 0; cycle < 5; cycle++) {
                assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, code), false));
                assertEquals(OPEN, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), false));
                assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), true));
                assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, code), true));
                // No down-edge Resume can remove the overlay before its up edge.
                assertEquals(CLOSE, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), true));
                assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), false));
            }
        }
    }

    @Test public void canceledAndRepeatedPressesDoNotToggle() {
        KorriOverlayHomeKey keys = new KorriOverlayHomeKey();
        int code = KeyEvent.KEYCODE_BUTTON_MODE;
        assertEquals(CONSUMED, keys.handle(new KeyEvent(0, 1, KeyEvent.ACTION_DOWN, code, 1), false));
        assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), false));
        assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, code), true));
        assertEquals(CONSUMED, keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, code), false));
        assertEquals(CLOSE, keys.handle(new KeyEvent(KeyEvent.ACTION_UP, code), false));
        keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, code), true);
        KeyEvent canceled = KeyEvent.changeFlags(new KeyEvent(KeyEvent.ACTION_UP, code), KeyEvent.FLAG_CANCELED);
        assertEquals(CONSUMED, keys.handle(canceled, true));
        assertEquals(UNHANDLED, keys.handle(new KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_DOWN), true));
    }
}
