package dev.korri.savebridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import java.io.File;
import java.nio.file.Files;
import java.util.ArrayList;
import org.junit.Test;

public class PathsTest {
    @Test
    public void relativePathsRefuseTraversal() {
        assertTrue(Paths.validRelative("save_0/game_data.sav"));
        for (String bad : new String[] {"", "/etc/passwd", "../x", "a/../b", "a//b", "a/./b", "a\nb"}) {
            assertFalse(bad, Paths.validRelative(bad));
        }
        assertTrue(Paths.validKey("save"));
        assertFalse(Paths.validKey("Save/1"));
    }

    @Test
    public void listsNestedFilesAndClears() throws Exception {
        File root = Files.createTempDirectory("bridge").toFile();
        new File(root, "save_0").mkdirs();
        Files.write(new File(root, "save_0/a.sav").toPath(), new byte[] {1});
        Files.write(new File(root, "option.sav").toPath(), new byte[] {2});
        ArrayList<String> files = new ArrayList<>();
        Paths.list(root, "", files);
        assertEquals(java.util.Arrays.asList("option.sav", "save_0/a.sav"), files);
        Paths.deleteChildren(root);
        assertEquals(0, root.listFiles().length);
    }
}
