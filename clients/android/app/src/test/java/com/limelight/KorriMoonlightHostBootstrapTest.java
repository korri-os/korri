package com.limelight;

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
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightHostBootstrapTest {
    @Test
    public void configuredNativePeersBecomeArtemisHostCandidates() throws Exception {
        List<KorriMoonlightHostBootstrap.Candidate> candidates =
                KorriMoonlightHostBootstrap.decodeCandidates(
                        "{\"_tag\":\"Candidates\",\"items\":["
                                + "{\"label\":\"zao\",\"address\":\"100.114.19.92\"},"
                                + "{\"label\":\"desk\",\"address\":\"desk.example\"}]}"
                );

        assertEquals(2, candidates.size());
        assertEquals("zao", candidates.get(0).label);
        assertEquals("100.114.19.92", candidates.get(0).address);
        assertEquals("desk", candidates.get(1).label);
        assertEquals("desk.example", candidates.get(1).address);
    }

    @Test(expected = IllegalArgumentException.class)
    public void malformedCandidateResponseIsRejected() throws Exception {
        KorriMoonlightHostBootstrap.decodeCandidates(
                "{\"_tag\":\"Candidates\",\"items\":[{\"label\":\"zao\"}]}"
        );
    }

    @Test
    public void successfulBootstrapWakesPortalOnce() {
        List<String> registered = new ArrayList<>();
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                () -> Arrays.asList(
                        new KorriMoonlightHostBootstrap.Candidate("zao", "100.114.19.92"),
                        new KorriMoonlightHostBootstrap.Candidate("desk", "desk.example"),
                        new KorriMoonlightHostBootstrap.Candidate("ipv6", "[2001:db8::1]")),
                (candidate, guard) -> Boolean.TRUE.equals(guard.commit(() -> {
                    registered.add(candidate.label + "@" + candidate.address);
                    return true;
                })),
                completions::incrementAndGet,
                Runnable::run);

        bootstrap.start();

        assertEquals(Arrays.asList(
                "zao@100.114.19.92", "desk@desk.example", "ipv6@[2001:db8::1]"), registered);
        assertEquals("zao", bootstrap.labelForAddress("100.114.19.92", 47989));
        assertEquals("desk", bootstrap.labelForAddress("DESK.EXAMPLE", 47989));
        assertEquals("ipv6", bootstrap.labelForAddress("2001:db8::1", 47989));
        assertNull(bootstrap.labelForAddress("100.114.19.92", 48000));
        assertEquals(1, completions.get());
        bootstrap.close();
    }

    @Test
    public void failedCandidatesDoNotSuppressLaterPeers() {
        List<String> registered = new ArrayList<>();
        AtomicInteger completions = new AtomicInteger();
        KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                () -> Arrays.asList(
                        new KorriMoonlightHostBootstrap.Candidate("offline", "192.0.2.1"),
                        new KorriMoonlightHostBootstrap.Candidate("zao", "100.114.19.92")),
                (candidate, guard) -> {
                    if (candidate.label.equals("offline")) throw new Exception("unreachable");
                    return Boolean.TRUE.equals(guard.commit(() -> {
                        registered.add(candidate.label);
                        return true;
                    }));
                },
                completions::incrementAndGet,
                Runnable::run);

        bootstrap.start();

        assertEquals(Arrays.asList("zao"), registered);
        assertEquals(1, completions.get());
        bootstrap.close();
    }

    @Test
    public void closeSuppressesCompletionFromAnInFlightBootstrap() throws Exception {
        CountDownLatch entered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger completions = new AtomicInteger();
        AtomicInteger commits = new AtomicInteger();
        ExecutorService executor = Executors.newSingleThreadExecutor();
        try {
            KorriMoonlightHostBootstrap bootstrap = new KorriMoonlightHostBootstrap(
                    () -> Arrays.asList(
                            new KorriMoonlightHostBootstrap.Candidate(
                                    "zao", "100.114.19.92")),
                    (candidate, guard) -> {
                        entered.countDown();
                        release.await(2, TimeUnit.SECONDS);
                        return Boolean.TRUE.equals(guard.commit(() -> {
                            commits.incrementAndGet();
                            return true;
                        }));
                    },
                    completions::incrementAndGet,
                    executor);

            bootstrap.start();
            assertTrue(entered.await(2, TimeUnit.SECONDS));
            bootstrap.close();
            release.countDown();
            executor.shutdown();
            assertTrue(executor.awaitTermination(2, TimeUnit.SECONDS));
            assertEquals(0, commits.get());
            assertEquals(0, completions.get());
        } finally {
            release.countDown();
            executor.shutdownNow();
        }
    }
}
