package com.limelight;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.stream.Stream;

public class KorriPairingCeremonyRemovalContractTest {
    private static final List<String> FORBIDDEN_JAVA = List.of(
            "PairingManager",
            "executePairingCommand(",
            "executePairingChallenge(",
            "generatePinString(",
            "getPairingManager(",
            "openPairing(",
            "NotPaired",
            "NOT_PAIRED",
            "doPair(",
            "doUnpair(");

    private static final List<String> FORBIDDEN_RESOURCES = List.of(
            "pairing",
            "pair_pc",
            "pair_otp",
            "pair_passphrase",
            "pair_pin",
            "unpair",
            "not_paired");

    @Test
    public void productionAndroidContainsNoGameStreamPairingCeremony() throws Exception {
        assertTreeContainsNone(Path.of("src/main/java"), ".java", FORBIDDEN_JAVA);
        assertTreeContainsNone(Path.of("src/main/res"), ".xml", FORBIDDEN_RESOURCES);
    }

    @Test
    public void publicBridgeAndPortalContainNoPairingState() throws Exception {
        Path repository = Path.of("../../..").normalize();
        List<String> forbidden = List.of(
                "openPairing(", "NotPaired", "readonly paired", ".paired", "paired:");
        assertTypeScriptTreeContainsNone(
                repository.resolve("clients/portal/src"), forbidden);
        assertTypeScriptTreeContainsNone(
                repository.resolve("contracts/bridge"), forbidden);
        try (Stream<Path> surfaces = Files.list(repository.resolve("surfaces"))) {
            for (Path surface : surfaces.filter(path -> Files.isDirectory(path.resolve("src"))).toList()) {
                assertTypeScriptTreeContainsNone(surface.resolve("src"), forbidden);
            }
        }
    }

    @Test
    public void secureMoonlightTransportRemainsPresent() throws Exception {
        String http = new String(Files.readAllBytes(
                Path.of("src/main/java/com/limelight/nvstream/http/NvHTTP.java")),
                StandardCharsets.UTF_8);
        String connection = new String(Files.readAllBytes(
                Path.of("src/main/java/com/limelight/nvstream/NvConnection.java")),
                StandardCharsets.UTF_8);
        assertTrue("TLS client key manager was removed", http.contains("X509KeyManager"));
        assertTrue("pinned server trust was removed", http.contains("serverCert"));
        assertTrue("PairStatus attestation was removed", http.contains("PairStatus"));
        assertTrue("HTTPS launch was removed", http.contains("getHttpsUrl(true)"));
        assertTrue("launch rikey was removed", http.contains("&rikey="));
        assertTrue("connection no longer checks certificate acceptance",
                connection.contains("getPairState(serverInfo)"));
    }

    private static void assertTreeContainsNone(
            Path root, String suffix, List<String> forbiddenValues) throws Exception {
        assertMatchingFilesContainNone(
                root,
                path -> path.toString().endsWith(suffix),
                forbiddenValues);
    }

    private static void assertTypeScriptTreeContainsNone(
            Path root, List<String> forbiddenValues) throws Exception {
        assertMatchingFilesContainNone(
                root,
                path -> path.toString().endsWith(".ts")
                        || path.toString().endsWith(".tsx"),
                forbiddenValues);
    }

    private static void assertMatchingFilesContainNone(
            Path root,
            java.util.function.Predicate<Path> matches,
            List<String> forbiddenValues) throws Exception {
        try (Stream<Path> files = Files.walk(root)) {
            for (Path file : files.filter(matches).toList()) {
                String relative = root.relativize(file).toString().replace('\\', '/');
                String source = new String(
                        Files.readAllBytes(file), StandardCharsets.UTF_8);
                for (String forbidden : forbiddenValues) {
                    assertFalse(relative + " contains " + forbidden,
                            source.toLowerCase().contains(forbidden.toLowerCase()));
                }
            }
        }
    }
}
