package com.limelight;

import android.content.Context;

import androidx.test.core.app.ApplicationProvider;

import com.limelight.binding.crypto.AndroidCryptoProvider;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.nio.charset.StandardCharsets;
import java.security.cert.X509Certificate;
import java.util.Collections;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

@RunWith(RobolectricTestRunner.class)
public class KorriMoonlightProvisioningTest {
    private static String certificatePem() {
        Context context = ApplicationProvider.getApplicationContext();
        return new String(
                new AndroidCryptoProvider(context).getPemEncodedClientCertificate(),
                StandardCharsets.US_ASCII);
    }

    private static ComputerDetails computer(String uuid) {
        ComputerDetails value = new ComputerDetails();
        value.uuid = uuid;
        value.name = "Zao";
        value.state = ComputerDetails.State.ONLINE;
        return value;
    }

    @Test
    public void neverPairedHostReturnsExactCommittedRecordCertificateAndApps() throws Exception {
        String pem = certificatePem();
        ComputerDetails original = computer("host-uuid");
        MemoryStore store = new MemoryStore(original);
        NvApp app = new NvApp("Korri Stream", "current", 7, false);
        AtomicReference<String> forwarded = new AtomicReference<>();
        KorriMoonlightProvisioning provisioning = provisioning(
                pem, store, forwarded, app);

        KorriMoonlightProvisioning.Provisioned result =
                provisioning.provisionAndLoadApps("host-uuid");

        assertEquals(pem, forwarded.get());
        assertNotNull(result.serverCertificate);
        assertEquals(Collections.singletonList(app), result.apps);
        assertSame(store.current, result.computer);
        assertSame(store.current.serverCert, result.serverCertificate);
        assertEquals("<apps/>", store.rawApps);
    }

    @Test
    public void coalescedWaiterRefreshesTheExactCurrentRecord() throws Exception {
        String pem = certificatePem();
        MemoryStore store = new MemoryStore(computer("host-uuid"));
        NvApp app = new NvApp("Korri Stream", "current", 7, false);
        CountDownLatch provisionEntered = new CountDownLatch(1);
        CountDownLatch release = new CountDownLatch(1);
        AtomicInteger provisions = new AtomicInteger();
        KorriMoonlightProvisioning provisioning = new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> {
                    provisions.incrementAndGet();
                    provisionEntered.countDown();
                    assertTrue(release.await(2, TimeUnit.SECONDS));
                    return pem;
                },
                (candidate, certificate) -> true,
                (candidate, certificate) -> new KorriMoonlightProvisioning.AppList(
                        "<apps/>", Collections.singletonList(app)),
                store);
        AtomicReference<KorriMoonlightProvisioning.Provisioned> first = new AtomicReference<>();
        AtomicReference<KorriMoonlightProvisioning.Provisioned> second = new AtomicReference<>();
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread owner = new Thread(() -> call(provisioning, first, failure));
        Thread waiter = new Thread(() -> call(provisioning, second, failure));
        owner.start();
        assertTrue(provisionEntered.await(2, TimeUnit.SECONDS));
        waiter.start();
        release.countDown();
        owner.join(2000);
        waiter.join(2000);

        assertNull(failure.get());
        assertEquals(1, provisions.get());
        assertSame(store.current, first.get().computer);
        assertSame(store.current, second.get().computer);
        assertSame(store.current.serverCert, second.get().serverCertificate);
    }

    @Test
    public void stalePollGenerationRetriesInsteadOfOverwritingNewerApps() throws Exception {
        String pem = certificatePem();
        MemoryStore store = new MemoryStore(computer("host-uuid"));
        store.staleFirstCommit = true;
        AtomicInteger provisions = new AtomicInteger();
        KorriMoonlightProvisioning provisioning = new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> {
                    provisions.incrementAndGet();
                    return pem;
                },
                (candidate, certificate) -> true,
                (candidate, certificate) -> new KorriMoonlightProvisioning.AppList(
                        "provisioned", Collections.emptyList()),
                store);

        KorriMoonlightProvisioning.Provisioned result =
                provisioning.repairAndLoadApps("host-uuid");

        assertEquals(2, provisions.get());
        assertEquals("provisioned", store.rawApps);
        assertSame(store.current, result.computer);
    }

    @Test
    public void cancelledLifecycleNeverCommitsPinOrApps() throws Exception {
        String pem = certificatePem();
        MemoryStore store = new MemoryStore(computer("host-uuid"));
        AtomicBoolean current = new AtomicBoolean(true);
        KorriMoonlightProvisioning provisioning = new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> pem,
                (candidate, certificate) -> true,
                (candidate, certificate) -> {
                    current.set(false);
                    return new KorriMoonlightProvisioning.AppList(
                            "<apps/>", Collections.emptyList());
                },
                store);

        try {
            provisioning.repairAndLoadApps("host-uuid", current::get);
            fail("expected cancellation");
        } catch (KorriMoonlightProvisioning.Failure expected) {
            assertEquals("ProvisioningCancelled", expected.reason);
        }
        assertNull(store.current.serverCert);
        assertNull(store.rawApps);
        assertEquals(0, store.commits.get());
    }

    @Test
    public void lifecycleCloseWaitsForFinalPinAndCacheCommitLease() throws Exception {
        String pem = certificatePem();
        MemoryStore store = new MemoryStore(computer("host-uuid"));
        store.beforeMutation = new CountDownLatch(1);
        store.releaseMutation = new CountDownLatch(1);
        LeasingGuard guard = new LeasingGuard();
        KorriMoonlightProvisioning provisioning = new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> pem,
                (candidate, certificate) -> true,
                (candidate, certificate) -> new KorriMoonlightProvisioning.AppList(
                        "<apps/>", Collections.emptyList()),
                store);
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread worker = new Thread(() -> {
            try {
                provisioning.repairAndLoadApps("host-uuid", guard);
            } catch (Throwable error) {
                failure.set(error);
            }
        });
        CountDownLatch closeReturned = new CountDownLatch(1);
        Thread closer = new Thread(() -> {
            guard.close();
            closeReturned.countDown();
        });

        worker.start();
        assertTrue(store.beforeMutation.await(2, TimeUnit.SECONDS));
        closer.start();
        assertFalse(closeReturned.await(100, TimeUnit.MILLISECONDS));
        store.releaseMutation.countDown();
        assertTrue(closeReturned.await(2, TimeUnit.SECONDS));
        worker.join(2000);
        closer.join(2000);

        assertNull(failure.get());
        assertNotNull(store.current.serverCert);
        assertEquals("<apps/>", store.rawApps);
        assertEquals(1, store.commits.get());
        try {
            provisioning.repairAndLoadApps("host-uuid", guard);
            fail("expected cancellation after close");
        } catch (KorriMoonlightProvisioning.Failure expected) {
            assertEquals("ProvisioningCancelled", expected.reason);
        }
        assertEquals(1, store.commits.get());
    }

    @Test
    public void invalidServerCertificateNeverCommits() throws Exception {
        String pem = certificatePem();
        MemoryStore store = new MemoryStore(computer("host-uuid"));
        KorriMoonlightProvisioning provisioning = new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> "not a certificate",
                (candidate, certificate) -> true,
                (candidate, certificate) -> {
                    throw new AssertionError("must not fetch apps");
                },
                store);
        try {
            provisioning.repairAndLoadApps("host-uuid");
            fail("expected invalid certificate");
        } catch (KorriMoonlightProvisioning.Failure expected) {
            assertEquals("InvalidCertificate", expected.reason);
            assertTrue(!expected.getMessage().contains("BEGIN CERTIFICATE"));
        }
        assertEquals(0, store.commits.get());
    }

    private static KorriMoonlightProvisioning provisioning(
            String pem,
            MemoryStore store,
            AtomicReference<String> forwarded,
            NvApp app) {
        return new KorriMoonlightProvisioning(
                () -> pem,
                (uuid, client) -> {
                    forwarded.set(client);
                    return pem;
                },
                (candidate, certificate) -> true,
                (candidate, certificate) -> new KorriMoonlightProvisioning.AppList(
                        "<apps/>", Collections.singletonList(app)),
                store);
    }

    private static void call(
            KorriMoonlightProvisioning provisioning,
            AtomicReference<KorriMoonlightProvisioning.Provisioned> result,
            AtomicReference<Throwable> failure) {
        try {
            result.set(provisioning.repairAndLoadApps("host-uuid"));
        } catch (Throwable error) {
            failure.compareAndSet(null, error);
        }
    }

    private static final class LeasingGuard implements KorriMoonlightDiscovery.Guard {
        private boolean current = true;

        @Override
        public synchronized boolean current() {
            return current;
        }

        @Override
        public synchronized <T> T commit(KorriMoonlightDiscovery.Commit<T> action)
                throws Exception {
            return current ? action.run() : null;
        }

        synchronized void close() {
            current = false;
        }
    }

    private static final class MemoryStore implements KorriMoonlightProvisioning.HostStateStore {
        private ComputerDetails current;
        private long generation;
        private String rawApps;
        private boolean staleFirstCommit;
        private CountDownLatch beforeMutation;
        private CountDownLatch releaseMutation;
        private final AtomicInteger commits = new AtomicInteger();

        private MemoryStore(ComputerDetails initial) {
            current = initial;
        }

        @Override
        public synchronized KorriMoonlightProvisioning.HostSnapshot snapshot(String hostUuid) {
            return new KorriMoonlightProvisioning.HostSnapshot(
                    new ComputerDetails(current), generation);
        }

        @Override
        public synchronized ComputerDetails current(String hostUuid) {
            return current;
        }

        @Override
        public synchronized KorriMoonlightProvisioning.HostCommit commit(
                KorriMoonlightProvisioning.HostSnapshot snapshot,
                X509Certificate serverCertificate,
                String rawAppList,
                KorriMoonlightDiscovery.Guard guard) {
            if (!guard.current()) {
                return new KorriMoonlightProvisioning.HostCommit(false, false, null);
            }
            commits.incrementAndGet();
            if (beforeMutation != null) {
                beforeMutation.countDown();
                try {
                    assertTrue(releaseMutation.await(2, TimeUnit.SECONDS));
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    throw new AssertionError(error);
                }
            }
            if (staleFirstCommit) {
                staleFirstCommit = false;
                rawApps = "newer-poller-apps";
                generation++;
                return new KorriMoonlightProvisioning.HostCommit(
                        false, true, new ComputerDetails(current));
            }
            if (snapshot.generation != generation) {
                return new KorriMoonlightProvisioning.HostCommit(
                        false, true, new ComputerDetails(current));
            }
            current.serverCert = serverCertificate;
            rawApps = rawAppList;
            generation++;
            return new KorriMoonlightProvisioning.HostCommit(true, false, current);
        }
    }
}
