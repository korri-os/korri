package com.limelight;

import android.content.pm.PackageManager;
import android.os.BatteryManager;
import android.content.Context;
import com.limelight.korri.moonlight.KorriMoonlightActionExecutor;
import com.limelight.korri.moonlight.KorriMoonlightActionExecutor.Effect;
import org.json.JSONArray;
import org.json.JSONObject;

/** UI-thread projection and effects extracted from the old Moonlight overlay. */
final class KorriViewerControls {
    private static boolean touchOnly(Effect effect) {
        switch (effect) {
            case TOGGLE_KEYBOARD: case TOGGLE_FULL_KEYBOARD: case SET_ZOOM_MODE:
            case SET_MOUSE_MODE: case TOGGLE_KEYBOARD_CONTROLLER: case SWITCH_TOUCH_SENSITIVITY:
                return true;
            default: return false;
        }
    }

    static boolean included(Effect effect, boolean touch) {
        if (!touch && touchOnly(effect)) return false;
        switch (effect) {
            case SET_FILL_MODE: case TOGGLE_KEYBOARD: case TOGGLE_FULL_KEYBOARD:
            case SET_ZOOM_MODE: case SET_MOUSE_MODE: case SET_LOCAL_CURSOR: case ROTATE_SCREEN:
            case TOGGLE_KEYBOARD_CONTROLLER: case SWITCH_TOUCH_SENSITIVITY:
            case SET_SGSR_SHARPNESS: case SET_SGSR_EDGE_THRESHOLD:
            case SET_FACE_BUTTON_FLIP: case SET_RUMBLE: case SET_PICTURE_IN_PICTURE:
                return true;
            default: return false;
        }
    }

    private static boolean supported(Game game, Effect effect) {
        boolean touch = game.getPackageManager().hasSystemFeature(PackageManager.FEATURE_TOUCHSCREEN);
        if (!included(effect, touch) || !game.available(effect)) return false;
        switch (effect) {
            case SET_SGSR_SHARPNESS: case SET_SGSR_EDGE_THRESHOLD: return game.viewerSgsrActive();
            case SET_LOCAL_CURSOR: return game.viewerHasPointer();
            case SET_RUMBLE: return game.viewerHasRumble();
            case ROTATE_SCREEN: return game.getPackageManager().hasSystemFeature(PackageManager.FEATURE_SCREEN_PORTRAIT)
                    && game.getPackageManager().hasSystemFeature(PackageManager.FEATURE_SCREEN_LANDSCAPE);
            default: return true;
        }
    }

    static String prefix(String launch, long generation, long lease) {
        return "viewer:" + launch + ":" + generation + ":" + lease + ":";
    }

    static String snapshot(Game game, String prefix) throws Exception {
        JSONArray controls = new JSONArray();
        for (Effect effect : Effect.values()) {
            if (!supported(game, effect)) continue;
            controls.put(new JSONObject().put("id", prefix + effect.wire())
                    .put("label", label(effect)).put("enabled", true).put("destructive", false)
                    .put("dismissOnSuccess", dismiss(effect)).put("interaction", interaction(game, effect)));
        }
        JSONObject result = new JSONObject().put("controls", controls);
        BatteryManager battery = (BatteryManager) game.getSystemService(Context.BATTERY_SERVICE);
        int capacity = battery == null ? Integer.MIN_VALUE : battery.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY);
        if (capacity >= 0 && capacity <= 100) result.put("batteryPercent", capacity);
        JSONObject health = game.viewerHealth();
        if (health != null) {
            // Legacy StreamHealthSample.sampledAtMs stays inside the viewer adapter.
            result.put("sampledAtMs", health.remove("sampledAtMs"));
            result.put("streamHealth", health);
        }
        return result.toString();
    }

    static boolean dismiss(Effect effect) {
        switch (effect) {
            case TOGGLE_KEYBOARD: case TOGGLE_FULL_KEYBOARD: case SET_ZOOM_MODE:
            case TOGGLE_KEYBOARD_CONTROLLER: case ROTATE_SCREEN: case SET_LOCAL_CURSOR:
                return true;
            default: return false;
        }
    }

    private static String label(Effect effect) {
        switch (effect) {
            case SET_FILL_MODE: return "Crop to fill";
            case TOGGLE_KEYBOARD: return "Toggle keyboard";
            case TOGGLE_FULL_KEYBOARD: return "Full keyboard";
            case SET_ZOOM_MODE: return "Pan & zoom";
            case SET_MOUSE_MODE: return "Mouse mode";
            case SET_LOCAL_CURSOR: return "Toggle local cursor";
            case ROTATE_SCREEN: return "Rotate screen";
            case TOGGLE_KEYBOARD_CONTROLLER: return "Keyboard as controller";
            case SWITCH_TOUCH_SENSITIVITY: return "Touch sensitivity";
            case SET_SGSR_SHARPNESS: return "SGSR sharpness";
            case SET_SGSR_EDGE_THRESHOLD: return "SGSR edge threshold";
            case SET_FACE_BUTTON_FLIP: return "Flip A/B and X/Y";
            case SET_RUMBLE: return "Rumble";
            case SET_PICTURE_IN_PICTURE: return "Picture-in-picture";
            default: throw new IllegalArgumentException("Unsupported stream control");
        }
    }

    private static JSONObject interaction(Game game, Effect effect) throws Exception {
        switch (effect) {
            case SET_FILL_MODE: return toggle(game.fillMode());
            case SET_ZOOM_MODE: return toggle(game.zoomMode());
            case SET_FACE_BUTTON_FLIP: return toggle(game.faceButtonFlip());
            case SET_RUMBLE: return toggle(game.rumble());
            case SET_PICTURE_IN_PICTURE: return toggle(game.pictureInPicture());
            case SET_SGSR_SHARPNESS: return range(game.sgsrSharpness(), 0, 50);
            case SET_SGSR_EDGE_THRESHOLD: return range(game.sgsrEdgeThreshold(), 1, 32);
            case SET_MOUSE_MODE:
                JSONArray options = new JSONArray();
                String[] labels = game.getMouseModeLabels();
                for (int i = 0; i < labels.length; i++) options.put(new JSONObject().put("value", String.valueOf(i)).put("label", labels[i]));
                return new JSONObject().put("kind", "choice").put("value", game.mouseMode()).put("options", options);
            default: return new JSONObject().put("kind", "command");
        }
    }

    private static JSONObject toggle(boolean value) throws Exception {
        return new JSONObject().put("kind", "toggle").put("value", value).put("trueLabel", "On").put("falseLabel", "Off");
    }
    private static JSONObject range(int value, int min, int max) throws Exception {
        return new JSONObject().put("kind", "range").put("value", value).put("min", min).put("max", max).put("step", 1);
    }

    static boolean apply(Game game, String launch, String prefix, String id, String json, KorriStreamControlState state) throws Exception {
        if (!id.startsWith(prefix)) return false;
        Effect effect = Effect.fromWire(id.substring(prefix.length()));
        if (effect == null || !supported(game, effect)) return false;
        JSONObject value = "null".equals(json) ? null : new JSONObject(json);
        KorriMoonlightActionExecutor.Request request;
        Object raw = null;
        if (value == null) request = KorriMoonlightActionExecutor.Request.command(launch, effect);
        else {
            raw = value.get("value");
            switch (value.getString("kind")) {
                case "toggle":
                    if (!(raw instanceof Boolean)) return false;
                    request = KorriMoonlightActionExecutor.Request.toggle(launch, effect, (Boolean) raw); break;
                case "choice":
                    if (!(raw instanceof String)) return false;
                    request = KorriMoonlightActionExecutor.Request.choice(launch, effect, (String) raw); break;
                case "range":
                    if (!(raw instanceof Number) || !Double.isFinite(((Number) raw).doubleValue())
                            || ((Number) raw).doubleValue() != ((Number) raw).intValue()) return false;
                    raw = ((Number) raw).intValue();
                    request = KorriMoonlightActionExecutor.Request.range(launch, effect, (Integer) raw); break;
                default: return false;
            }
        }
        KorriMoonlightActionExecutor executor = new KorriMoonlightActionExecutor(game, new KorriMoonlightActionExecutor.UiDispatcher() {
            public boolean isUiThread() { return true; }
            public void dispatch(Runnable action) { throw new IllegalStateException("Expected UI thread"); }
        });
        if (executor.executeLocal(request) != KorriMoonlightActionExecutor.Outcome.EXECUTED) return false;
        switch (effect) {
            case SET_FILL_MODE: raw = game.viewerScaleMode(); break;
            case TOGGLE_FULL_KEYBOARD: raw = game.isKeyboardLayoutVisible(); break;
            case TOGGLE_KEYBOARD_CONTROLLER: raw = game.viewerKeyboardControllerVisible(); break;
            case SWITCH_TOUCH_SENSITIVITY: raw = game.viewerTouchSensitivity(); break;
            case ROTATE_SCREEN: raw = game.viewerOrientation(); break;
            case SET_LOCAL_CURSOR: raw = game.localCursor(); break;
            case TOGGLE_KEYBOARD: return true; // IME visibility belongs to the current Android window.
            default: break;
        }
        state.remember(effect, raw);
        // The live mouse operation also changes zoom; never replay an old zoom value.
        if (effect == Effect.SET_MOUSE_MODE) state.remember(Effect.SET_ZOOM_MODE, game.zoomMode());
        return true;
    }

    static void restore(Game game, KorriStreamControlState state) {
        state.restore((effect, value) -> {
            switch (effect) {
                case SET_FILL_MODE: game.viewerRestoreScaleMode((com.limelight.preferences.PreferenceConfiguration.ScaleMode) value); break;
                case SET_ZOOM_MODE: game.setZoomMode((Boolean) value); break;
                case SET_MOUSE_MODE: game.setMouseMode((String) value); break;
                case SET_SGSR_SHARPNESS: game.setSgsrSharpness((Integer) value); break;
                case SET_SGSR_EDGE_THRESHOLD: game.setSgsrEdgeThreshold((Integer) value); break;
                case SET_FACE_BUTTON_FLIP: game.setFaceButtonFlip((Boolean) value); break;
                case SET_RUMBLE: game.setRumble((Boolean) value); break;
                case SET_PICTURE_IN_PICTURE: game.setPictureInPicture((Boolean) value); break;
                case TOGGLE_FULL_KEYBOARD: if (game.isKeyboardLayoutVisible() != (Boolean) value) game.toggleFullKeyboard(); break;
                case TOGGLE_KEYBOARD_CONTROLLER: if (game.viewerKeyboardControllerVisible() != (Boolean) value) game.toggleKeyboardController(); break;
                case SWITCH_TOUCH_SENSITIVITY: if (game.viewerTouchSensitivity() != (Boolean) value) game.switchTouchSensitivity(); break;
                case ROTATE_SCREEN: if (game.viewerOrientation() != (Integer) value) game.rotateScreen(); break;
                case SET_LOCAL_CURSOR: if (game.localCursor() != (Boolean) value) game.toggleLocalCursor(); break;
                default: throw new IllegalArgumentException("Not stream-local state");
            }
        });
    }
    private KorriViewerControls() { }
}
