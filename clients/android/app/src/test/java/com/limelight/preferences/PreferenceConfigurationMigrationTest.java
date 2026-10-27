package com.limelight.preferences;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;

import android.content.Context;
import android.content.SharedPreferences;

import androidx.preference.PreferenceManager;
import androidx.test.core.app.ApplicationProvider;


import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;


@Config(sdk = 33)
@RunWith(RobolectricTestRunner.class)
public class PreferenceConfigurationMigrationTest {
    private SharedPreferences basePrefs;

    @Before
    public void setUp() {
        Context context = ApplicationProvider.getApplicationContext();
        basePrefs = PreferenceManager.getDefaultSharedPreferences(context);
        basePrefs.edit().clear().commit();
    }

    @Test
    public void migratesLegacyOpacityWhenKeyboardOpacityIsAbsent() {
        basePrefs.edit().putInt("seekbar_osc_opacity", 25).commit();

        PreferenceConfiguration.migrateLegacyKeyboardOpacity(basePrefs);

        assertEquals(25, basePrefs.getInt("seekbar_keyboard_axi_opacity", -1));
        assertFalse(basePrefs.contains("seekbar_osc_opacity"));
    }

    @Test
    public void keepsExistingKeyboardOpacityAndRemovesLegacyKey() {
        basePrefs.edit()
                .putInt("seekbar_keyboard_axi_opacity", 70)
                .putInt("seekbar_osc_opacity", 25)
                .commit();

        PreferenceConfiguration.migrateLegacyKeyboardOpacity(basePrefs);

        assertEquals(70, basePrefs.getInt("seekbar_keyboard_axi_opacity", -1));
        assertFalse(basePrefs.contains("seekbar_osc_opacity"));
    }

    @Test
    public void resolvesLegacyOpacityAheadOfCurrentKeyWithoutWriting() {
        basePrefs.edit()
                .putInt("seekbar_keyboard_axi_opacity", 70)
                .putInt("seekbar_osc_opacity", 25)
                .commit();

        // The read path is separate from migration: it must honour the legacy
        // key when present and leave the store untouched.
        assertEquals(25, PreferenceConfiguration.resolveKeyboardOpacity(basePrefs));
        assertEquals(70, basePrefs.getInt("seekbar_keyboard_axi_opacity", -1));
        assertEquals(25, basePrefs.getInt("seekbar_osc_opacity", -1));
    }

    @Test
    public void migratesRemovedStereoRenderModesToTwoD() {
        for (String removed : new String[]{"1", "2"}) {
            basePrefs.edit().putString("render_mode_list", removed).commit();

            PreferenceConfiguration.migrateRemovedStereoRenderModes(basePrefs);

            assertEquals("0", basePrefs.getString("render_mode_list", null));
        }
    }

    @Test
    public void preservesSgsrRenderModeAcrossMigration() {
        basePrefs.edit().putString("render_mode_list", "3").commit();

        PreferenceConfiguration.migrateRemovedStereoRenderModes(basePrefs);

        assertEquals("3", basePrefs.getString("render_mode_list", null));
        assertEquals(3, PreferenceConfiguration.resolveRenderMode(basePrefs));
    }

    @Test
    public void resolvesRemovedStereoRenderModeWithoutWriting() {
        basePrefs.edit().putString("render_mode_list", "2").commit();

        // Reading a removed mode reports 2D; only migration rewrites the store.
        assertEquals(0, PreferenceConfiguration.resolveRenderMode(basePrefs));
        assertEquals("2", basePrefs.getString("render_mode_list", null));
    }
}
