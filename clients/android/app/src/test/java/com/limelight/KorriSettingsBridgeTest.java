package com.limelight;

import android.content.Context;
import android.content.SharedPreferences;
import android.preference.PreferenceManager;

import androidx.test.core.app.ApplicationProvider;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;

/**
 * Contract tests for the theme-free Korri settings bridge.
 * Any web surface renders from schemaJson/valuesJson and writes via
 * applySetting; these tests pin that contract.
 */
@RunWith(RobolectricTestRunner.class)
public class KorriSettingsBridgeTest {

    private Context context;
    private SharedPreferences prefs;

    @Before
    public void setUp() {
        context = ApplicationProvider.getApplicationContext();
        prefs = PreferenceManager.getDefaultSharedPreferences(context);
        prefs.edit().clear().apply();
    }

    private JSONObject findSetting(JSONObject schema, String key) throws Exception {
        JSONArray categories = schema.getJSONArray("categories");
        for (int c = 0; c < categories.length(); c++) {
            JSONArray settings = categories.getJSONObject(c).getJSONArray("settings");
            for (int s = 0; s < settings.length(); s++) {
                JSONObject setting = settings.getJSONObject(s);
                if (setting.getString("key").equals(key)) return setting;
            }
        }
        return null;
    }

    @Test
    public void schemaIsValidAndVersioned() throws Exception {
        JSONObject schema = new JSONObject(KorriSettingsBridge.schemaJson(context));
        assertEquals(1, schema.getInt("schemaVersion"));
        assertTrue(schema.getJSONArray("categories").length() >= 4);
        // every setting has key/type/title/default
        JSONArray categories = schema.getJSONArray("categories");
        for (int c = 0; c < categories.length(); c++) {
            JSONArray settings = categories.getJSONObject(c).getJSONArray("settings");
            for (int s = 0; s < settings.length(); s++) {
                JSONObject setting = settings.getJSONObject(s);
                assertNotNull(setting.getString("key"));
                assertNotNull(setting.getString("type"));
                assertNotNull(setting.getString("title"));
                assertTrue(setting.has("default"));
            }
        }
    }

    @Test
    public void valuesReturnDefaultsOnFreshInstall() throws Exception {
        JSONObject values = new JSONObject(KorriSettingsBridge.valuesJson(context));
        assertEquals("1280x720", values.getString("list_resolution"));
        assertEquals("60", values.getString("list_fps"));
        assertEquals("latency", values.getString("frame_pacing"));
        assertEquals(20, values.getInt("seekbar_sgsr_sharpness"));
        assertFalse(values.getBoolean("checkbox_enable_hdr"));
        assertTrue(values.getBoolean("checkbox_enable_quit_dialog"));
    }

    @Test
    public void removedRenderModeIsReportedAsTwoD() throws Exception {
        prefs.edit().putString("render_mode_list", "2").commit();

        JSONObject values = new JSONObject(KorriSettingsBridge.valuesJson(context));

        assertEquals("0", values.getString("render_mode_list"));
    }

    @Test
    public void sgsrRenderModeSurvivesAsStoredValue() throws Exception {
        prefs.edit().putString("render_mode_list", "3").commit();

        JSONObject values = new JSONObject(KorriSettingsBridge.valuesJson(context));

        assertEquals("3", values.getString("render_mode_list"));
    }

    @Test
    public void toggleWritesBooleanPreference() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "checkbox_enable_hdr", "true"));
        assertEquals("ok", result.getString("status"));
        assertTrue(prefs.getBoolean("checkbox_enable_hdr", false));
        assertTrue(result.getJSONObject("values").getBoolean("checkbox_enable_hdr"));
    }

    @Test
    public void choiceWritesStringPreference() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "list_video_scale_mode", "\"fill\""));
        assertEquals("ok", result.getString("status"));
        assertEquals("fill", prefs.getString("list_video_scale_mode", null));
    }

    @Test
    public void choiceRejectsUnknownOption() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "video_format", "\"forceVP9\""));
        assertEquals("error", result.getString("status"));
        assertFalse(prefs.contains("video_format"));
    }

    @Test
    public void rangeClampsAndWritesIntPreference() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "seekbar_sgsr_sharpness", "999"));
        assertEquals("ok", result.getString("status"));
        assertEquals(50, prefs.getInt("seekbar_sgsr_sharpness", -1)); // clamped to max
    }

    @Test
    public void unknownKeyIsRejected() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "not_a_real_key", "true"));
        assertEquals("error", result.getString("status"));
    }

    @Test
    public void resolutionChangeCascadesDefaultBitrate() throws Exception {
        JSONObject result = new JSONObject(
                KorriSettingsBridge.applySetting(context, "list_resolution", "\"1920x1080\""));
        assertEquals("ok", result.getString("status"));
        int expected = com.limelight.preferences.PreferenceConfiguration
                .getDefaultBitrate("1920x1080", "60");
        assertEquals(expected, prefs.getInt("seekbar_bitrate_kbps", -1));
        assertEquals(expected, result.getJSONObject("values").getInt("seekbar_bitrate_kbps"));
    }

    @Test
    public void writesLandOnExactKeysAndTypesPreferenceConfigurationReads() throws Exception {
        // Full readPreferences() can't run on the JVM (loads moonlight-core JNI),
        // so pin the raw key/type contract it consumes instead.
        KorriSettingsBridge.applySetting(context, "list_resolution", "\"1920x1080\"");
        KorriSettingsBridge.applySetting(context, "render_mode_list", "\"3\"");
        KorriSettingsBridge.applySetting(context, "seekbar_sgsr_sharpness", "35");
        // PreferenceConfiguration reads: getString("list_resolution"), getString("render_mode_list"),
        // getInt("seekbar_sgsr_sharpness") / 10f  -- see readPreferences().
        assertEquals("1920x1080", prefs.getString("list_resolution", null));
        assertEquals("3", prefs.getString("render_mode_list", null));
        assertEquals(35, prefs.getInt("seekbar_sgsr_sharpness", -1));
    }
}
