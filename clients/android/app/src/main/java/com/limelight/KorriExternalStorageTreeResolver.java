package com.limelight;

import android.net.Uri;

import java.io.File;
import java.util.List;

/** Pure path resolver for Android ExternalStorageProvider tree selections. */
final class KorriExternalStorageTreeResolver {
    static final String EXTERNAL_STORAGE_PROVIDER = "com.android.externalstorage.documents";

    private KorriExternalStorageTreeResolver() {}

    static Result resolve(String treeUri, List<Volume> volumes) {
        Uri uri = Uri.parse(treeUri);
        if (!EXTERNAL_STORAGE_PROVIDER.equals(uri.getAuthority())) {
            return Result.problem(
                    "FolderSelectionProviderUnsupported",
                    "Choose a folder from Android device storage, not a cloud or app provider");
        }
        List<String> segments = uri.getPathSegments();
        int treeIndex = segments.indexOf("tree");
        if (treeIndex < 0 || treeIndex + 1 >= segments.size()) {
            return Result.problem("FolderSelectionUnresolvable", "Android did not return a folder identity");
        }
        String documentId = segments.get(treeIndex + 1);
        int separator = documentId.indexOf(':');
        if (separator < 0) {
            return Result.problem("FolderSelectionUnresolvable", "Android folder identity has no volume");
        }
        String volumeId = documentId.substring(0, separator);
        String relative = documentId.substring(separator + 1);
        if (relative.isEmpty()) {
            return Result.problem("FolderSelectionRootForbidden", "Choose a folder inside the volume, not the volume root");
        }
        Volume volume = null;
        for (Volume candidate : volumes) {
            if (candidate.id.equals(volumeId)) {
                volume = candidate;
                break;
            }
        }
        if (volume == null) {
            return Result.problem("FolderSelectionUnresolvable", "Android storage volume is not available");
        }
        try {
            File root = volume.root.getCanonicalFile();
            File selected = new File(root, relative).getCanonicalFile();
            String rootPath = root.getPath();
            String selectedPath = selected.getPath();
            if (selectedPath.equals(rootPath)) {
                return Result.problem("FolderSelectionRootForbidden", "Choose a folder inside the volume, not the volume root");
            }
            if (!selectedPath.startsWith(rootPath + File.separator)) {
                return Result.problem("FolderSelectionEscapedVolume", "Selected folder is outside the Android storage volume");
            }
            if (!selected.isDirectory()) {
                return Result.problem("FolderSelectionUnresolvable", "Selected folder is not available");
            }
            return Result.ok(selected);
        } catch (Exception error) {
            return Result.problem(
                    "FolderSelectionUnresolvable",
                    error.getMessage() != null ? error.getMessage() : "Selected folder is not available");
        }
    }

    static final class Volume {
        final String id;
        final File root;

        Volume(String id, File root) {
            this.id = id;
            this.root = root;
        }
    }

    static final class Result {
        final File canonicalDirectory;
        final String code;
        final String message;

        private Result(File canonicalDirectory, String code, String message) {
            this.canonicalDirectory = canonicalDirectory;
            this.code = code;
            this.message = message;
        }

        boolean isOk() {
            return canonicalDirectory != null;
        }

        static Result ok(File canonicalDirectory) {
            return new Result(canonicalDirectory, null, null);
        }

        static Result problem(String code, String message) {
            return new Result(null, code, message);
        }
    }
}
