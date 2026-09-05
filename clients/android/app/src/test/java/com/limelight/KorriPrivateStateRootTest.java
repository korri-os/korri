package com.limelight;

import android.content.Context;
import android.content.ContextWrapper;
import android.content.Intent;

import androidx.test.core.app.ApplicationProvider;

import com.simonwjackson.korri.korrid.KorriBrainService;

import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.io.File;
import java.nio.file.Files;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriPrivateStateRootTest {
    @Test
    public void serviceRestartIntentCarriesStablePrivateRootSeparateFromReadableRoot() {
        Context context = ApplicationProvider.getApplicationContext();
        String readableRoot = "/storage/emulated/0/korri";

        Intent intent = KorriBrainService.launchIntent(
                context, "https://appassets.androidplatform.net", readableRoot);

        String privateRoot = intent.getStringExtra("privateStateRoot");
        assertEquals("https://appassets.androidplatform.net", intent.getStringExtra("allowedOrigin"));
        assertEquals(readableRoot, intent.getStringExtra("localStorageRoot"));
        assertEquals(KorriBrainService.privateStateRoot(context), privateRoot);
        assertFalse(privateRoot.equals(readableRoot));
        assertTrue(privateRoot.contains(context.getPackageName()));
    }

    @Rule
    public TemporaryFolder temporary = new TemporaryFolder();

    private Context storageContext(File noBackup) {
        return new ContextWrapper(null) {
            @Override
            public Context getApplicationContext() {
                return this;
            }

            @Override
            public File getNoBackupFilesDir() {
                return noBackup;
            }
        };
    }

    @Test
    public void resolvesThePlatformUserDirectoryBeforePassingPrivateStorageToRust() throws Exception {
        File userData = temporary.newFolder("data");
        File userAlias = new File(temporary.getRoot(), "user-zero");
        Files.createSymbolicLink(userAlias.toPath(), userData.toPath());
        File noBackup = new File(userAlias, "app/no_backup");
        Files.createDirectories(noBackup.toPath());

        String root = KorriBrainService.privateStateRoot(storageContext(noBackup));

        assertEquals(new File(userData, "app/no_backup/korrid-state").getAbsolutePath(), root);
        assertEquals(new File(root).getCanonicalPath(), root);
    }

    @Test
    public void keepsAppOwnedSymlinksVisibleToRustValidation() throws Exception {
        for (String linkName : new String[]{"app", "no_backup", "korrid-state"}) {
            File userData = temporary.newFolder(linkName);
            File target = temporary.newFolder(linkName + "-target");
            File appData = new File(userData, "app");
            File noBackup = new File(appData, "no_backup");
            File privateRoot = new File(noBackup, "korrid-state");
            File link = linkName.equals("app") ? appData
                    : linkName.equals("no_backup") ? noBackup : privateRoot;
            Files.createDirectories(link.getParentFile().toPath());
            Files.createSymbolicLink(link.toPath(), target.toPath());
            Files.createDirectories(privateRoot.toPath());

            String root = KorriBrainService.privateStateRoot(storageContext(noBackup));

            assertEquals(privateRoot.getAbsolutePath(), root);
            assertNotEquals(privateRoot.getCanonicalPath(), root);
        }
    }
}
