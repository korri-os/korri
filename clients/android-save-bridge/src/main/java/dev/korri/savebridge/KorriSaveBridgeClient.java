package dev.korri.savebridge;

import android.content.ContentResolver;
import android.content.Context;
import android.net.Uri;
import android.os.Bundle;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;

/**
 * Korri's side of the save bridge. korrid names the folders (its save root
 * and the runner's saveLocations keys); this class copies them through the
 * game APK's provider at session boundaries.
 */
public final class KorriSaveBridgeClient {
    private final ContentResolver resolver;
    private final String authority;

    private KorriSaveBridgeClient(ContentResolver resolver, String authority) {
        this.resolver = resolver;
        this.authority = authority;
    }

    /** The provider's authority is {@code <package>.korri.savebridge}. Null when the app has no bridge. */
    public static KorriSaveBridgeClient forPackage(Context context, String packageName) {
        String authority = packageName + ".korri.savebridge";
        if (context.getPackageManager().resolveContentProvider(authority, 0) == null) {
            return null;
        }
        return new KorriSaveBridgeClient(context.getContentResolver(), authority);
    }

    private Bundle call(String method, String key) throws IOException {
        Bundle out = resolver.call(Uri.parse("content://" + authority), method, key, null);
        if (out == null || !out.getBoolean("ok")) {
            throw new IOException(out == null ? method + " failed" : out.getString("error"));
        }
        return out;
    }

    private Uri uri(String key, String relative) {
        Uri.Builder b = new Uri.Builder().scheme("content").authority(authority).appendPath(key);
        for (String part : relative.split("/")) {
            b.appendPath(part);
        }
        return b.build();
    }

    /** Replace the game's folder with Korri's copy. */
    public void copyIn(String key, File from) throws IOException {
        call(KorriSaveBridgeProvider.METHOD_CLEAR, key);
        ArrayList<String> files = new ArrayList<>();
        Paths.list(from, "", files);
        for (String relative : files) {
            try (InputStream in = new FileInputStream(new File(from, relative));
                 OutputStream out = resolver.openOutputStream(uri(key, relative), "wt")) {
                copy(in, out);
            }
        }
    }

    /** Replace Korri's folder with the game's copy. Korri snapshots it next. */
    public void copyOut(String key, File to) throws IOException {
        ArrayList<String> files = call(KorriSaveBridgeProvider.METHOD_LIST, key).getStringArrayList("files");
        to.mkdirs();
        Paths.deleteChildren(to);
        for (String relative : files == null ? new ArrayList<String>() : files) {
            if (!Paths.validRelative(relative)) {
                throw new IOException("the game listed an invalid path");
            }
            File target = new File(to, relative);
            target.getParentFile().mkdirs();
            try (InputStream in = resolver.openInputStream(uri(key, relative));
                 OutputStream out = new FileOutputStream(target)) {
                copy(in, out);
            }
        }
    }

    public void quit() throws IOException {
        call(KorriSaveBridgeProvider.METHOD_QUIT, null);
    }

    private static void copy(InputStream in, OutputStream out) throws IOException {
        byte[] buffer = new byte[1 << 16];
        for (int n; (n = in.read(buffer)) > 0; ) {
            out.write(buffer, 0, n);
        }
    }
}
