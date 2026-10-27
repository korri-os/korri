package com.limelight;

import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertFalse;

@RunWith(RobolectricTestRunner.class)
public class KorriAtomicFileTest {
    @Rule
    public TemporaryFolder temporary = new TemporaryFolder();

    @Test
    public void replacesProvisionedContentWithoutLeavingRecoveryFiles() throws Exception {
        File target = temporary.newFile("retroarch.cfg");
        Files.write(target.toPath(), "old config".getBytes(StandardCharsets.UTF_8));

        KorriAtomicFile.write(target, "new config".getBytes(StandardCharsets.UTF_8));

        assertArrayEquals("new config".getBytes(StandardCharsets.UTF_8),
                Files.readAllBytes(target.toPath()));
        assertFalse(new File(target.getPath() + ".bak").exists());
    }
}
