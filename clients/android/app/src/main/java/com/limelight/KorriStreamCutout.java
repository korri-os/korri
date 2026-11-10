package com.limelight;

import android.os.Build;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;

import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

/**
 * The stream window draws through the camera cutout, as the shell does
 * (#388): ALWAYS on API 30+, SHORT_EDGES on API 28-29. It touches only the
 * cutout mode; the scale mode still decides how the picture fits the window.
 */
final class KorriStreamCutout {
    private KorriStreamCutout() {}

    static void drawThroughCutout(Window window) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return;
        WindowManager.LayoutParams attributes = window.getAttributes();
        attributes.layoutInDisplayCutoutMode = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                ? WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                : WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        window.setAttributes(attributes);
    }

    /**
     * Keeps an overlay drawn over the stream inside the cutout's safe area, so
     * its controls stay out of the camera hole; the stream under it still
     * reaches the edge. Follows rotation and folding, which change the insets.
     */
    static void keepOutOfCutout(View overlay) {
        ViewCompat.setOnApplyWindowInsetsListener(overlay, (view, insets) -> {
            Insets cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout());
            ViewGroup.MarginLayoutParams params = (ViewGroup.MarginLayoutParams) view.getLayoutParams();
            if (params.leftMargin != cutout.left || params.topMargin != cutout.top
                    || params.rightMargin != cutout.right || params.bottomMargin != cutout.bottom) {
                params.setMargins(cutout.left, cutout.top, cutout.right, cutout.bottom);
                view.setLayoutParams(params);
            }
            return insets;
        });
        ViewCompat.requestApplyInsets(overlay);
    }
}
