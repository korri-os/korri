package com.limelight;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.io.File;
import java.util.Arrays;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriGameFolderBridgeTest {
    @Test
    public void pickerStatePublishesReceiptUntilAcknowledged() throws Exception {
        KorriGameFolderPickerState state = new KorriGameFolderPickerState();
        JSONObject opened = new JSONObject(state.choose());
        assertEquals("Opened", opened.getString("_tag"));
        assertEquals("Choosing", new JSONObject(state.snapshotJson())
                .getJSONObject("state").getString("_tag"));

        state.selected("receipt-1");
        JSONObject selected = new JSONObject(state.snapshotJson());
        String generation = selected.getString("generation");
        assertEquals(1, selected.getInt("version"));
        assertEquals("Selected", selected.getJSONObject("state").getString("_tag"));
        assertEquals("receipt-1", selected.getJSONObject("state").getString("receipt"));
        assertFalse(selected.toString().contains("/storage/"));

        assertEquals("Acknowledged",
                new JSONObject(state.acknowledgeJson(generation)).getString("_tag"));
        assertEquals("Idle", new JSONObject(state.snapshotJson())
                .getJSONObject("state").getString("_tag"));
        assertEquals("Stale",
                new JSONObject(state.acknowledgeJson(generation)).getString("_tag"));
    }

    @Test
    public void pickerStateRejectsDuplicateOpenWhileChoosingOrSelected() throws Exception {
        KorriGameFolderPickerState state = new KorriGameFolderPickerState();
        JSONObject opened = new JSONObject(state.choose());
        JSONObject busyChoosing = new JSONObject(state.choose());
        assertEquals("Busy", busyChoosing.getString("_tag"));
        assertEquals(opened.getString("generation"), busyChoosing.getString("generation"));
        assertEquals("Choosing", busyChoosing.getString("state"));

        state.selected("receipt-1");
        JSONObject selected = new JSONObject(state.snapshotJson());
        JSONObject busySelected = new JSONObject(state.choose());
        assertEquals("Busy", busySelected.getString("_tag"));
        assertEquals(selected.getString("generation"), busySelected.getString("generation"));
        assertEquals("Selected", busySelected.getString("state"));
        assertEquals("receipt-1", new JSONObject(state.snapshotJson())
                .getJSONObject("state").getString("receipt"));
    }

    @Test
    public void pickerStatePublishesCancelledAndProblemAsDefinitiveGenerations() throws Exception {
        KorriGameFolderPickerState state = new KorriGameFolderPickerState();
        state.cancelled();
        JSONObject cancelled = new JSONObject(state.snapshotJson());
        assertEquals("Cancelled", cancelled.getJSONObject("state").getString("_tag"));
        state.acknowledgeJson(cancelled.getString("generation"));

        state.problem("FolderSelectionUnresolvable", "cloud folder");
        JSONObject problem = new JSONObject(state.snapshotJson());
        assertEquals("Problem", problem.getJSONObject("state").getString("_tag"));
        assertEquals("FolderSelectionUnresolvable",
                problem.getJSONObject("state").getString("code"));
    }

    @Test
    public void resolverAcceptsExternalStorageProviderSubfolderOnly() throws Exception {
        File volume = tempDirectory("primary-volume");
        File games = new File(volume, "Games/GBA");
        assertTrue(games.mkdirs());

        KorriExternalStorageTreeResolver.Result result = KorriExternalStorageTreeResolver.resolve(
                "content://com.android.externalstorage.documents/tree/primary%3AGames%2FGBA",
                Arrays.asList(new KorriExternalStorageTreeResolver.Volume("primary", volume)));

        assertTrue(result.isOk());
        assertEquals(games.getCanonicalFile(), result.canonicalDirectory);
    }

    @Test
    public void resolverRejectsCloudProviderVolumeRootAndEscapes() throws Exception {
        File volume = tempDirectory("primary-volume");
        assertProblem(
                "FolderSelectionProviderUnsupported",
                "content://com.example.cloud.documents/tree/primary%3AGames",
                volume);
        assertProblem(
                "FolderSelectionRootForbidden",
                "content://com.android.externalstorage.documents/tree/primary%3A",
                volume);
        assertProblem(
                "FolderSelectionEscapedVolume",
                "content://com.android.externalstorage.documents/tree/primary%3A..%2Fescape",
                volume);
    }

    @Test
    public void resolverRejectsUnavailableVolumeOrFolder() throws Exception {
        File volume = tempDirectory("primary-volume");
        assertProblem(
                "FolderSelectionUnresolvable",
                "content://com.android.externalstorage.documents/tree/1234-5678%3AGames",
                volume);
        assertProblem(
                "FolderSelectionUnresolvable",
                "content://com.android.externalstorage.documents/tree/primary%3AMissing",
                volume);
    }

    private static void assertProblem(String code, String uri, File volume) {
        KorriExternalStorageTreeResolver.Result result = KorriExternalStorageTreeResolver.resolve(
                uri,
                Arrays.asList(new KorriExternalStorageTreeResolver.Volume("primary", volume)));
        assertFalse(result.isOk());
        assertEquals(code, result.code);
    }

    private static File tempDirectory(String name) throws Exception {
        File root = java.nio.file.Files.createTempDirectory(name).toFile();
        root.deleteOnExit();
        return root;
    }
}
