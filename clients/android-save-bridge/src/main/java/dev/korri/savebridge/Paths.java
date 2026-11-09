package dev.korri.savebridge;

import java.io.File;
import java.io.IOException;
import java.util.List;

/** Path rules shared by both sides of the save bridge. Pure Java, unit-tested. */
public final class Paths {
    private Paths() {}

    public static boolean validKey(String key) {
        return key.matches("[a-z0-9][a-z0-9._-]{0,63}");
    }

    /** Relative, no empty, "." or ".." segment, no control characters. */
    public static boolean validRelative(String path) {
        if (path.isEmpty() || path.startsWith("/") || path.length() > 1024) {
            return false;
        }
        for (String part : path.split("/", -1)) {
            if (part.isEmpty() || part.equals(".") || part.equals("..")) {
                return false;
            }
            for (char c : part.toCharArray()) {
                if (c < 0x20 || c == 0x7f) {
                    return false;
                }
            }
        }
        return true;
    }

    /** Regular files under root, as relative paths. Symlinks are skipped. */
    public static void list(File root, String prefix, List<String> out) throws IOException {
        File[] children = root.listFiles();
        if (children == null) {
            return;
        }
        java.util.Arrays.sort(children);
        for (File child : children) {
            String name = prefix.isEmpty() ? child.getName() : prefix + "/" + child.getName();
            if (!child.getCanonicalPath().equals(child.getAbsolutePath())) {
                continue;
            }
            if (child.isDirectory()) {
                list(child, name, out);
            } else if (child.isFile()) {
                out.add(name);
            }
        }
    }

    public static void deleteChildren(File root) throws IOException {
        File[] children = root.listFiles();
        if (children == null) {
            return;
        }
        for (File child : children) {
            if (child.isDirectory() && child.getCanonicalPath().equals(child.getAbsolutePath())) {
                deleteChildren(child);
            }
            if (!child.delete()) {
                throw new IOException("cannot delete " + child.getName());
            }
        }
    }
}
