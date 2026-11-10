package com.limelight;

import com.limelight.korri.moonlight.KorriMoonlightActionExecutor.Effect;
import com.limelight.utils.PanZoomHandler;
import java.util.EnumMap;
import java.util.Map;

/** Stream-local overrides. Owned by KorriStreamViewer.Session, never SharedPreferences. */
final class KorriStreamControlState {
    interface Apply { void accept(Effect effect, Object value); }
    private final EnumMap<Effect, Object> values = new EnumMap<>(Effect.class);
    // The existing PanZoomHandler.setInitialZoomAndPan(scale, offsetX, offsetY) tuple.
    private static final class ZoomPan {
        final float scale, offsetX, offsetY;
        ZoomPan(PanZoomHandler handler) {
            scale = handler.getScaleFactor(); offsetX = handler.getChildX(); offsetY = handler.getChildY();
        }
    }
    private ZoomPan zoomPan;

    void captureZoomPan(PanZoomHandler handler) { zoomPan = new ZoomPan(handler); }
    void restoreZoomPan(PanZoomHandler handler) {
        if (zoomPan != null) handler.setInitialZoomAndPan(zoomPan.scale, zoomPan.offsetX, zoomPan.offsetY);
    }


    void remember(Effect effect, Object value) { values.put(effect, value); }

    void restore(Apply apply) {
        // Mouse mode resets zoom, so replay zoom after it (enum order is not effect order).
        for (Map.Entry<Effect, Object> entry : values.entrySet()) {
            if (entry.getKey() != Effect.SET_ZOOM_MODE) apply.accept(entry.getKey(), entry.getValue());
        }
        if (values.containsKey(Effect.SET_ZOOM_MODE)) {
            apply.accept(Effect.SET_ZOOM_MODE, values.get(Effect.SET_ZOOM_MODE));
        }
    }
}
