package com.limelight;

import android.content.Context;
import android.content.SharedPreferences;
import android.preference.PreferenceManager;

import com.limelight.preferences.PreferenceConfiguration;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Theme-free settings contract for Korri web surfaces.
 *
 * The web side owns presentation top-to-bottom. This bridge exposes only:
 *   - schemaJson():  what settings exist (key, type, constraints, options, cascades)
 *   - valuesJson():  current values, typed
 *   - applySetting(): validated write + native-parity cascades, returns fresh values
 *
 * Values are written to the exact SharedPreferences keys PreferenceConfiguration
 * reads, so the native stream runtime is untouched. Titles in the schema are
 * fallback labels; surfaces may re-label, re-group, or re-order freely.
 */
final class KorriSettingsBridge {

    private KorriSettingsBridge() {}

    static final int SCHEMA_VERSION = 1;

    // --- schema DSL -------------------------------------------------------

    private static JSONObject toggle(String key, String title, boolean def) throws JSONException {
        return new JSONObject().put("key", key).put("type", "toggle")
                .put("title", title).put("default", def);
    }

    private static JSONObject choice(String key, String title, String def,
                                     String[][] options, String[] affects) throws JSONException {
        JSONArray opts = new JSONArray();
        for (String[] pair : options) {
            opts.put(new JSONObject().put("value", pair[0]).put("label", pair[1]));
        }
        JSONObject setting = new JSONObject().put("key", key).put("type", "choice")
                .put("title", title).put("default", def).put("options", opts);
        if (affects != null && affects.length > 0) {
            setting.put("affects", new JSONArray(java.util.Arrays.asList(affects)));
        }
        return setting;
    }

    private static JSONObject range(String key, String title, int def,
                                    int min, int max, int step, String unit) throws JSONException {
        return new JSONObject().put("key", key).put("type", "range")
                .put("title", title).put("default", def)
                .put("min", min).put("max", max).put("step", step).put("unit", unit);
    }

    private static JSONObject category(String id, String title, JSONObject... settings) throws JSONException {
        JSONArray arr = new JSONArray();
        for (JSONObject s : settings) arr.put(s);
        return new JSONObject().put("id", id).put("title", title).put("settings", arr);
    }

    // --- curated schema (keys + defaults mirror PreferenceConfiguration) ---

    private static JSONObject buildSchema(Context context) throws JSONException {
        JSONArray categories = new JSONArray();

        categories.put(category("video", "Video",
                choice("list_resolution", "Resolution", "1280x720", new String[][]{
                        {"640x360", "360p"}, {"854x480", "480p"}, {"1280x720", "720p"},
                        {"1920x1080", "1080p"}, {"2560x1440", "1440p"}, {"3840x2160", "4K"},
                }, new String[]{"seekbar_bitrate_kbps"}),
                choice("list_fps", "Frame rate", "60", new String[][]{
                        {"30", "30 FPS"}, {"60", "60 FPS"}, {"90", "90 FPS"}, {"120", "120 FPS"},
                }, new String[]{"seekbar_bitrate_kbps"}),
                range("seekbar_bitrate_kbps", "Bitrate",
                        PreferenceConfiguration.getDefaultBitrate(context), 500, 150000, 500, "kbps"),
                choice("video_format", "Codec", "auto", new String[][]{
                        {"auto", "Auto"}, {"forceav1", "Force AV1"},
                        {"forceh265", "Force HEVC"}, {"neverh265", "Never HEVC"},
                }, null),
                choice("list_video_scale_mode", "Scale mode", "fit", new String[][]{
                        {"fit", "Fit (letterbox)"}, {"fill", "Crop to fill"}, {"stretch", "Stretch"},
                }, null),
                choice("frame_pacing", "Frame pacing", "latency", new String[][]{
                        {"warp2", "Warp 2 (experimental)"}, {"warp", "Warp Drive (experimental)"},
                        {"latency", "Lowest latency"}, {"balanced", "Balanced"},
                        {"cap-fps", "Balanced with FPS limit"}, {"smoothness", "Smoothest video"},
                }, null),
                choice("render_mode_list", "Render mode", "0", new String[][]{
                        {"0", "Default (2D)"}, {"3", "SGSR upscale"},
                }, null),
                range("seekbar_sgsr_sharpness", "SGSR edge sharpness", 20, 0, 50, 1, "x0.1").put("live", true),
                range("seekbar_sgsr_edge_threshold", "SGSR edge threshold", 8, 1, 32, 1, "/255").put("live", true),
                toggle("checkbox_enable_hdr", "HDR (experimental)", false),
                toggle("checkbox_full_range", "Full range color", false),
                toggle("checkbox_ultra_low_latency", "Ultra low latency mode", false),
                toggle("checkbox_reduce_refresh_rate", "Reduce refresh rate", false),
                toggle("checkbox_enforce_display_mode", "Enforce display mode", false),
                toggle("checkbox_enable_pip", "Picture-in-picture", false).put("live", true),
                toggle("checkbox_enable_sops", "Optimize host game settings (SOPS)", true),
                toggle("checkbox_unlock_fps", "Unlock FPS limit", false),
                toggle("checkbox_prevent_packet_loss", "Prevent packet loss", false),
                // Apollo-only features (virtual display, resolution scale,
                // custom refresh rate) are intentionally absent. The enabled
                // Moonlight plugin owns Korri's Sunshine app selection.
                toggle("checkbox_auto_orientation", "Auto orientation", false),
                toggle("checkbox_auto_invert_video_resolution", "Auto invert resolution in portrait", true),
                toggle("checkbox_full_screen", "Hide system bars (full screen)", true)));

        categories.put(category("audio", "Audio",
                choice("list_audio_config", "Channels", "2", new String[][]{
                        {"2", "Stereo"}, {"51", "5.1 surround"}, {"71", "7.1 surround"},
                }, null),
                toggle("checkbox_host_audio", "Keep audio on host PC", false),
                toggle("checkbox_enable_audiofx", "Audio effects (AudioFX)", false)));

        categories.put(category("input", "Input",
                toggle("checkbox_multi_controller", "Multiple controllers", true),
                toggle("checkbox_enable_rumble", "Rumble", true).put("live", true),
                toggle("checkbox_gamepad_motion_sensors", "Motion sensors", true),
                toggle("checkbox_flip_face_buttons", "Flip A/B and X/Y", false).put("live", true),
                toggle("checkbox_absolute_mouse_mode", "Absolute mouse mode", false),
                range("seekbar_deadzone", "Stick deadzone", 5, 0, 50, 1, "%"),
                toggle("checkbox_mouse_emulation", "Start-hold mouse emulation", true).put("live", true),
                toggle("checkbox_gamepad_touchpad_as_mouse", "Gamepad touchpad as mouse", false).put("live", true),
                toggle("checkbox_enable_device_rumble", "Rumble on this device", false).put("live", true),
                toggle("checkbox_back_as_guide", "Back button as Guide", false).put("live", true),
                toggle("checkbox_back_as_meta", "Back button as Meta/Win", false).put("live", true),
                range("seekbar_trackpad_sensitivity_x", "Trackpad sensitivity X", 100, -200, 200, 10, "%"),
                range("seekbar_trackpad_sensitivity_y", "Trackpad sensitivity Y", 100, -200, 200, 10, "%")));

        categories.put(category("overlay", "In-game overlay",
                toggle("checkbox_enable_quit_dialog", "In-game menu (Back / Start hold)", true).put("live", true),
                toggle("checkbox_enable_floating_button", "Floating menu button", false).put("live", true),
                toggle("checkbox_disable_warnings", "Hide warning toasts", false).put("live", true)));

        return new JSONObject()
                .put("schemaVersion", SCHEMA_VERSION)
                .put("categories", categories);
    }

    // --- bridge entry points ------------------------------------------------

    static String schemaJson(Context context) {
        try {
            return buildSchema(context).toString();
        } catch (JSONException e) {
            return errorJson(e);
        }
    }

    static String valuesJson(Context context) {
        try {
            return readValues(context).toString();
        } catch (JSONException e) {
            return errorJson(e);
        }
    }

    /** Validated write. Returns {"status":"ok","values":{...}} with cascades applied. */
    static String applySetting(Context context, String key, String rawJsonValue) {
        try {
            JSONObject setting = findSetting(context, key);
            if (setting == null) {
                return new JSONObject().put("status", "error")
                        .put("message", "unknown setting: " + key).toString();
            }
            SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
            SharedPreferences.Editor editor = prefs.edit();
            // Values arrive as JSON so types survive the string bridge.
            Object value = new JSONObject("{\"v\":" + rawJsonValue + "}").get("v");

            switch (setting.getString("type")) {
                case "toggle":
                    editor.putBoolean(key, toBoolean(value));
                    break;
                case "choice": {
                    String candidate = String.valueOf(value);
                    if (!isValidOption(setting, candidate)) {
                        return new JSONObject().put("status", "error")
                                .put("message", "invalid option for " + key + ": " + candidate).toString();
                    }
                    editor.putString(key, candidate);
                    break;
                }
                case "range": {
                    int intValue = (int) Math.round(Double.parseDouble(String.valueOf(value)));
                    int min = setting.getInt("min");
                    int max = setting.getInt("max");
                    editor.putInt(key, Math.max(min, Math.min(max, intValue)));
                    break;
                }
                default:
                    return new JSONObject().put("status", "error")
                            .put("message", "unsupported type").toString();
            }
            editor.apply();

            // Native-parity cascade: resolution/fps changes re-derive default bitrate,
            // matching StreamSettings' listener behavior.
            if (key.equals("list_resolution") || key.equals("list_fps")) {
                String res = prefs.getString("list_resolution", "1280x720");
                String fps = prefs.getString("list_fps", "60");
                prefs.edit().putInt("seekbar_bitrate_kbps",
                        PreferenceConfiguration.getDefaultBitrate(res, fps)).apply();
            }

            return new JSONObject().put("status", "ok")
                    .put("values", readValues(context)).toString();
        } catch (Exception e) {
            return errorJson(e);
        }
    }

    // --- helpers -------------------------------------------------------------

    private static JSONObject readValues(Context context) throws JSONException {
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        JSONObject values = new JSONObject();
        JSONArray categories = buildSchema(context).getJSONArray("categories");
        for (int c = 0; c < categories.length(); c++) {
            JSONArray settings = categories.getJSONObject(c).getJSONArray("settings");
            for (int s = 0; s < settings.length(); s++) {
                JSONObject setting = settings.getJSONObject(s);
                String key = setting.getString("key");
                switch (setting.getString("type")) {
                    case "toggle":
                        values.put(key, prefs.getBoolean(key, setting.getBoolean("default")));
                        break;
                    case "choice":
                        values.put(key, readChoice(prefs, setting, key));
                        break;
                    case "range":
                        values.put(key, prefs.getInt(key, setting.getInt("default")));
                        break;
                }
            }
        }
        return values;
    }

    /**
     * The schema is the contract: a stored value that is no longer one of the
     * offered options (a setting whose feature was removed, say) falls back to
     * the default rather than crossing the bridge as a choice the portal
     * cannot render.
     */
    private static String readChoice(SharedPreferences prefs, JSONObject setting, String key)
            throws JSONException {
        String def = setting.getString("default");
        String stored = prefs.getString(key, def);
        JSONArray options = setting.getJSONArray("options");
        for (int i = 0; i < options.length(); i++) {
            if (options.getJSONObject(i).getString("value").equals(stored)) {
                return stored;
            }
        }
        return def;
    }

    private static JSONObject findSetting(Context context, String key) throws JSONException {
        JSONArray categories = buildSchema(context).getJSONArray("categories");
        for (int c = 0; c < categories.length(); c++) {
            JSONArray settings = categories.getJSONObject(c).getJSONArray("settings");
            for (int s = 0; s < settings.length(); s++) {
                JSONObject setting = settings.getJSONObject(s);
                if (setting.getString("key").equals(key)) return setting;
            }
        }
        return null;
    }

    private static boolean isValidOption(JSONObject setting, String candidate) throws JSONException {
        JSONArray options = setting.getJSONArray("options");
        for (int i = 0; i < options.length(); i++) {
            if (options.getJSONObject(i).getString("value").equals(candidate)) return true;
        }
        return false;
    }

    private static boolean toBoolean(Object value) {
        if (value instanceof Boolean) return (Boolean) value;
        return Boolean.parseBoolean(String.valueOf(value));
    }

    private static String errorJson(Exception e) {
        try {
            return new JSONObject().put("status", "error")
                    .put("message", String.valueOf(e.getMessage())).toString();
        } catch (JSONException impossible) {
            return "{\"status\":\"error\",\"message\":\"json failure\"}";
        }
    }
}
