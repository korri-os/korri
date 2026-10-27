package com.limelight;

import android.util.AtomicFile;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;

final class KorriAtomicFile {
    private KorriAtomicFile() {
    }

    static void write(File target, byte[] content) throws IOException {
        AtomicFile atomicFile = new AtomicFile(target);
        FileOutputStream output = null;
        try {
            output = atomicFile.startWrite();
            output.write(content);
            atomicFile.finishWrite(output);
        } catch (IOException | RuntimeException error) {
            if (output != null) {
                atomicFile.failWrite(output);
            }
            throw error;
        }
    }
}
