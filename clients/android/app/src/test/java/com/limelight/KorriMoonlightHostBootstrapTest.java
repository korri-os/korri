package com.limelight;

import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvHTTP;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightHostBootstrapTest {
    @Test
    public void configuredNativePeersBecomeArtemisHostCandidates() {
        List<KorriMoonlightHostBootstrap.Candidate> candidates =
                KorriMoonlightHostBootstrap.decodeCandidates(
                        "{\"_tag\":\"Candidates\",\"items\":["
                                + "{\"label\":\"zao\",\"address\":\"zao:48000\"},"
                                + "{\"label\":\"desk\",\"address\":\"desk.example\"}]}"
                );

        assertEquals(2, candidates.size());
        assertEquals("zao", candidates.get(0).label);
        assertEquals("zao", candidates.get(0).manualAddress.address);
        assertEquals(48000, candidates.get(0).manualAddress.port);
        assertEquals("desk.example", candidates.get(1).manualAddress.address);
        assertEquals(NvHTTP.DEFAULT_HTTP_PORT, candidates.get(1).manualAddress.port);
    }

    @Test
    public void malformedPeerBesideValidPeerIsOmitted() {
        List<KorriMoonlightHostBootstrap.Candidate> candidates =
                KorriMoonlightHostBootstrap.decodeCandidates(
                        "{\"_tag\":\"Candidates\",\"items\":["
                                + "{\"label\":\"broken\",\"address\":\"\"},"
                                + "{\"label\":\"missing\"},"
                                + "{\"label\":\"zao\",\"address\":\"127.0.0.1:47989\"}]}"
                );

        assertEquals(1, candidates.size());
        assertEquals("zao", candidates.get(0).label);
    }

    @Test
    public void noValidCandidateReturnsStableFailure() {
        try {
            KorriMoonlightHostBootstrap.decodeCandidates(
                    "{\"_tag\":\"Candidates\",\"items\":[{\"label\":\"zao\"}]}"
            );
            fail("expected invalid candidates");
        } catch (IllegalArgumentException error) {
            assertEquals("no valid Moonlight host candidates", error.getMessage());
        }
    }

    @Test
    public void bracketedIpv6AndNonDefaultPortsArePreserved() {
        ComputerDetails.AddressTuple ipv6 = KorriMoonlightAddressParser.parse("[::1]:48001");
        ComputerDetails.AddressTuple defaulted = KorriMoonlightAddressParser.parse("[::1]");

        assertEquals("::1", ipv6.address);
        assertEquals(48001, ipv6.port);
        assertEquals("::1", defaulted.address);
        assertEquals(NvHTTP.DEFAULT_HTTP_PORT, defaulted.port);
    }

    @Test
    public void permissionDeniedCandidateReadRetriesWithoutBrainRestart() {
        AtomicInteger reads = new AtomicInteger();
        List<String> registered = new ArrayList<>();
        KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                () -> {
                    if (reads.getAndIncrement() == 0) throw new SecurityException("denied");
                    return Arrays.asList(candidate("zao", "zao:47989"));
                },
                candidate -> {
                    registered.add(candidate.label);
                    return true;
                },
                () -> { },
                Runnable::run);

        bootstrap.start();
        bootstrap.start();

        assertEquals(2, reads.get());
        assertEquals(Arrays.asList("zao"), registered);
        bootstrap.close();
    }

    @Test
    public void successfulHostIsNotRegisteredTwiceAcrossLifecycleRetries() {
        AtomicInteger registrations = new AtomicInteger();
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                () -> Arrays.asList(candidate("zao", "zao:47989")),
                candidate -> {
                    registrations.incrementAndGet();
                    return true;
                },
                completions::incrementAndGet,
                Runnable::run);

        bootstrap.start();
        bootstrap.start();

        assertEquals(1, registrations.get());
        assertEquals(1, completions.get());
        bootstrap.close();
    }

    @Test
    public void failedCandidatesDoNotWakePortal() {
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                () -> Arrays.asList(candidate("zao", "zao:47989")),
                candidate -> false,
                completions::incrementAndGet,
                Runnable::run);

        bootstrap.start();

        assertEquals(0, completions.get());
        bootstrap.close();
    }

    @Test
    public void closeSuppressesCompletionFromAnInFlightBootstrap() throws Exception {
        CountDownLatch entered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger completions = new AtomicInteger();
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                    () -> Arrays.asList(candidate("zao", "zao:47989")),
                    candidate -> {
                        entered.countDown();
                        release.await(2, TimeUnit.SECONDS);
                        return true;
                    },
                    completions::incrementAndGet,
                    executor);

            bootstrap.start();
            assertTrue(entered.await(2, TimeUnit.SECONDS));
            bootstrap.close();
            release.countDown();
            executor.shutdown();
            assertTrue(executor.awaitTermination(2, TimeUnit.SECONDS));
            assertEquals(0, completions.get());
        } finally {
            release.countDown();
            executor.shutdownNow();
        }
    }

    private static KorriMoonlightHostBootstrap.Candidate candidate(
            String label, String address) {
        return new KorriMoonlightHostBootstrap.Candidate(
                label, KorriMoonlightAddressParser.parse(address));
    }
}
