package com.limelight.identity;

import org.junit.Test;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class KorriPersonSignerContractTest {
    @Test
    public void manifestDeclaresNip55VisibilityWithoutNamingOneSignerPackage() throws Exception {
        String manifest = read("src/main/AndroidManifest.xml");
        assertTrue(manifest.contains("<data android:scheme=\"nostrsigner\" />"));
        assertFalse(manifest.contains("greenart7c3"));
        assertFalse(manifest.contains("amber"));
    }

    @Test
    public void shellUsesTheActivityResultLifecycleAndKeepsEarlyDelivery() throws Exception {
        String shell = read("src/main/java/com/limelight/KorriShellActivity.java");
        assertTrue(shell.contains("registerForActivityResult("));
        assertTrue(shell.contains("new ActivityResultContracts.StartActivityForResult()"));
        assertTrue(shell.contains("pendingSignerActivityResult = result;"));
        assertTrue(shell.contains("ownerBindingController.onActivityResult("));
    }

    @Test
    public void jniCarriesOnlyPublicIdentityDataAndUnsignedOrSignedEvents() throws Exception {
        String java = read("src/main/java/com/simonwjackson/korri/korrid/KorridServer.java");
        String rust = read("../../../services/korrid/src/android.rs");
        assertTrue(java.contains("ownerBindingTemplate(long createdAt)"));
        assertTrue(java.contains("String unsignedTemplateJson"));
        assertTrue(java.contains("String signedEventJson"));
        assertTrue(rust.contains("local_owner_binding_template"));
        assertTrue(rust.contains("apply_local_owner_binding"));
        assertFalse(java.toLowerCase().contains("personprivate"));
        assertFalse(java.toLowerCase().contains("nsec"));
    }

    @Test
    public void pendingStateIsPersistedBeforeAndroidLaunchesTheSigner() throws Exception {
        String source = read("src/main/java/com/limelight/identity/Nip55PersonSigner.java");
        int persist = source.indexOf("persistPending(PHASE_ACCOUNT");
        int launch = source.indexOf("return launch(intent, \"Choose a signer account\")");
        assertTrue(persist >= 0);
        assertTrue(launch > persist);
        assertTrue(source.contains("REQUEST_TIMEOUT_MS = 120_000L"));
        assertTrue(source.contains("intent.setPackage(signerPackage)"));
        assertTrue(source.contains("resolver.query("));
    }

    private static String read(String path) throws Exception {
        return new String(Files.readAllBytes(Path.of(path)), StandardCharsets.UTF_8);
    }
}
