package com.limelight;

import android.content.Context;

import com.limelight.binding.PlatformBinding;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.nvstream.http.HostCertificateState;
import com.limelight.utils.ServerHelper;
import com.simonwjackson.korri.korrid.KorridServer;

import org.json.JSONObject;

import java.io.ByteArrayInputStream;
import java.io.StringReader;
import java.nio.charset.StandardCharsets;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.ConcurrentHashMap;

/** Provisions Moonlight trust through korrid before Artemis uses HTTPS. */
final class KorriMoonlightProvisioning {
    private static final int MAX_CERTIFICATE_BYTES = 12_288;
    private static final int MAX_COMMIT_ATTEMPTS = 2;

    static final class HostSnapshot {
        final ComputerDetails computer;
        final long generation;

        HostSnapshot(ComputerDetails computer, long generation) {
            this.computer = computer;
            this.generation = generation;
        }
    }

    static final class HostCommit {
        final boolean committed;
        final boolean stale;
        final ComputerDetails computer;

        HostCommit(boolean committed, boolean stale, ComputerDetails computer) {
            this.committed = committed;
            this.stale = stale;
            this.computer = computer;
        }
    }

    static final class Provisioned {
        final ComputerDetails computer;
        final X509Certificate serverCertificate;
        final String rawAppList;
        final List<NvApp> apps;

        Provisioned(
                ComputerDetails computer,
                X509Certificate serverCertificate,
                String rawAppList,
                List<NvApp> apps) {
            this.computer = computer;
            this.serverCertificate = serverCertificate;
            this.rawAppList = rawAppList;
            this.apps = apps;
        }
    }

    private static final class InFlight {
        private boolean complete;
        private Provisioned result;
        private Failure failure;

        synchronized Provisioned await() throws Failure {
            while (!complete) {
                try {
                    wait();
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    throw new Failure(
                            "ProvisioningFailed", "certificate provisioning was interrupted");
                }
            }
            if (failure != null) throw failure;
            return result;
        }

        synchronized void succeed(Provisioned value) {
            result = value;
            complete = true;
            notifyAll();
        }

        synchronized void fail(Failure error) {
            failure = error;
            complete = true;
            notifyAll();
        }
    }

    interface ClientCertificateSource {
        String read() throws Exception;
    }

    interface Provisioner {
        String provision(String hostUuid, String publicClientCertificate) throws Exception;
    }

    interface Attestor {
        boolean isAccepted(ComputerDetails computer, X509Certificate serverCertificate)
                throws Exception;
    }

    interface AppListSource {
        AppList fetch(ComputerDetails computer, X509Certificate serverCertificate)
                throws Exception;
    }

    interface HostStateStore {
        HostSnapshot snapshot(String hostUuid) throws Exception;
        ComputerDetails current(String hostUuid) throws Exception;
        HostCommit commit(
                HostSnapshot snapshot,
                X509Certificate serverCertificate,
                String rawAppList,
                KorriMoonlightDiscovery.Guard guard) throws Exception;
    }

    static final class AppList {
        final String raw;
        final List<NvApp> apps;

        AppList(String raw, List<NvApp> apps) {
            this.raw = raw;
            this.apps = apps;
        }
    }

    static final class Failure extends Exception {
        final String reason;

        Failure(String reason, String message) {
            super(message);
            this.reason = reason;
        }
    }

    private final ClientCertificateSource clientCertificateSource;
    private final Provisioner provisioner;
    private final Attestor attestor;
    private final AppListSource appListSource;
    private final HostStateStore hostStateStore;
    private final ConcurrentHashMap<String, InFlight> inFlight = new ConcurrentHashMap<>();

    KorriMoonlightProvisioning(
            ClientCertificateSource clientCertificateSource,
            Provisioner provisioner,
            Attestor attestor,
            AppListSource appListSource,
            HostStateStore hostStateStore) {
        this.clientCertificateSource = clientCertificateSource;
        this.provisioner = provisioner;
        this.attestor = attestor;
        this.appListSource = appListSource;
        this.hostStateStore = hostStateStore;
    }

    static KorriMoonlightProvisioning artemis(
            Context context,
            ComputerManagerService.ComputerManagerBinder binder) {
        Context application = context.getApplicationContext();
        String uniqueId = binder.getUniqueId();
        return new KorriMoonlightProvisioning(
                () -> {
                    byte[] pem = PlatformBinding.getCryptoProvider(application)
                            .getPemEncodedClientCertificate();
                    if (pem == null) {
                        throw new IllegalStateException("client certificate is unavailable");
                    }
                    return new String(pem, StandardCharsets.US_ASCII);
                },
                (hostUuid, publicClientCertificate) -> {
                    String result = KorridServer.provisionMoonlightCertificate(
                            hostUuid, publicClientCertificate);
                    JSONObject response = new JSONObject(result);
                    if (!"Provisioned".equals(response.optString("_tag"))) {
                        String code = response.optString("code", "ProvisioningFailed");
                        throw new IllegalStateException(
                                "certificate provisioning failed: " + code);
                    }
                    return response.getString("serverCertificate");
                },
                (computer, serverCertificate) -> http(
                        application, uniqueId, computer, serverCertificate).getPairState()
                        == HostCertificateState.ACCEPTED,
                (computer, serverCertificate) -> {
                    String raw = http(application, uniqueId, computer, serverCertificate)
                            .getAppListRaw();
                    return new AppList(
                            raw, NvHTTP.getAppListByReader(new StringReader(raw)));
                },
                new HostStateStore() {
                    @Override
                    public HostSnapshot snapshot(String hostUuid) {
                        ComputerManagerService.ComputerManagerBinder.MoonlightHostSnapshot value =
                                binder.snapshotMoonlightHost(hostUuid);
                        return value == null
                                ? null
                                : new HostSnapshot(value.computer, value.generation);
                    }

                    @Override
                    public ComputerDetails current(String hostUuid) {
                        return binder.getComputer(hostUuid);
                    }

                    @Override
                    public HostCommit commit(
                            HostSnapshot snapshot,
                            X509Certificate serverCertificate,
                            String rawAppList,
                            KorriMoonlightDiscovery.Guard guard) throws Exception {
                        ComputerManagerService.ComputerManagerBinder.MoonlightHostCommit value =
                                binder.commitMoonlightHost(
                                        snapshot.computer.uuid,
                                        snapshot.generation,
                                        serverCertificate,
                                        rawAppList,
                                        guard::current);
                        return new HostCommit(value.committed, value.stale, value.computer);
                    }
                });
    }

    private static NvHTTP http(
            Context context,
            String uniqueId,
            ComputerDetails source,
            X509Certificate serverCertificate) throws Exception {
        ComputerDetails candidate = new ComputerDetails(source);
        candidate.serverCert = serverCertificate;
        return new NvHTTP(
                ServerHelper.getCurrentAddressFromComputer(candidate),
                candidate.httpsPort,
                uniqueId,
                serverCertificate,
                PlatformBinding.getCryptoProvider(context));
    }

    Provisioned provisionAndLoadApps(String hostUuid) throws Failure {
        return loadApps(hostUuid, false, () -> true);
    }

    Provisioned repairAndLoadApps(String hostUuid) throws Failure {
        return loadApps(hostUuid, true, () -> true);
    }

    Provisioned repairAndLoadApps(String hostUuid, KorriMoonlightDiscovery.Guard guard)
            throws Failure {
        return loadApps(hostUuid, true, guard);
    }

    private Provisioned loadApps(
            String hostUuid, boolean forceProvision, KorriMoonlightDiscovery.Guard guard)
            throws Failure {
        if (hostUuid == null || hostUuid.trim().isEmpty()) {
            throw new Failure("ProvisioningFailed", "host identity is unavailable");
        }
        if (!guard.current()) throw cancelled();

        InFlight owner = new InFlight();
        InFlight active = inFlight.putIfAbsent(hostUuid, owner);
        if (active != null) {
            Provisioned result = active.await();
            return refreshCurrent(result, forceProvision, guard);
        }
        try {
            Provisioned result = loadAppsOwned(hostUuid, forceProvision, guard);
            owner.succeed(result);
            return result;
        } catch (Failure failure) {
            owner.fail(failure);
            throw failure;
        } finally {
            inFlight.remove(hostUuid, owner);
        }
    }

    private Provisioned refreshCurrent(
            Provisioned result,
            boolean forceProvision,
            KorriMoonlightDiscovery.Guard guard) throws Failure {
        try {
            if (!guard.current()) throw cancelled();
            ComputerDetails current = hostStateStore.current(result.computer.uuid);
            if (current == null) {
                throw new Failure("ProvisioningFailed", "computer disappeared during provisioning");
            }
            if (sameCertificate(current.serverCert, result.serverCertificate)) {
                return new Provisioned(
                        current,
                        result.serverCertificate,
                        result.rawAppList,
                        result.apps);
            }
            return loadAppsOwned(result.computer.uuid, forceProvision, guard);
        } catch (Failure failure) {
            throw failure;
        } catch (Exception error) {
            throw sanitizedFailure(error);
        }
    }

    private Provisioned loadAppsOwned(
            String hostUuid, boolean forceProvision, KorriMoonlightDiscovery.Guard guard)
            throws Failure {
        for (int attempt = 0; attempt < MAX_COMMIT_ATTEMPTS; attempt++) {
            try {
                if (!guard.current()) throw cancelled();
                HostSnapshot snapshot = hostStateStore.snapshot(hostUuid);
                if (snapshot == null || snapshot.computer == null) {
                    throw new Failure("ProvisioningFailed", "host is unavailable");
                }
                X509Certificate serverCertificate = snapshot.computer.serverCert;
                if (forceProvision || serverCertificate == null) {
                    String clientPem = requireSinglePem(clientCertificateSource.read());
                    serverCertificate = parseSingleCertificate(
                            provisioner.provision(hostUuid, clientPem));
                }
                if (serverCertificate == null) {
                    throw new Failure(
                            "ProvisioningFailed", "server certificate is unavailable");
                }
                if (!guard.current()) throw cancelled();
                if (!attestor.isAccepted(snapshot.computer, serverCertificate)) {
                    throw new Failure(
                            "HostCertificateRejected", "host did not accept the provisioned certificate");
                }
                AppList appList = appListSource.fetch(snapshot.computer, serverCertificate);
                if (appList == null || appList.raw == null || appList.apps == null) {
                    throw new Failure("AppListFailed", "host returned an invalid app list");
                }
                if (!guard.current()) throw cancelled();
                X509Certificate exactServerCertificate = serverCertificate;
                HostCommit committed = guard.commit(() -> hostStateStore.commit(
                        snapshot,
                        exactServerCertificate,
                        appList.raw,
                        () -> true));
                if (committed == null) throw cancelled();
                if (committed.committed && committed.computer != null) {
                    return new Provisioned(
                            committed.computer, serverCertificate, appList.raw, appList.apps);
                }
                if (!committed.stale) {
                    throw new Failure(
                            "ProvisioningFailed", "computer disappeared during provisioning");
                }
            } catch (Failure failure) {
                throw failure;
            } catch (Exception error) {
                throw sanitizedFailure(error);
            }
        }
        throw new Failure(
                "ProvisioningChanged", "host app state changed during provisioning; retry");
    }

    private static boolean sameCertificate(X509Certificate left, X509Certificate right)
            throws Exception {
        return left != null && right != null
                && Arrays.equals(left.getEncoded(), right.getEncoded());
    }

    private static Failure cancelled() {
        return new Failure("ProvisioningCancelled", "certificate provisioning was cancelled");
    }

    private static Failure sanitizedFailure(Exception error) {
        String message = error.getMessage();
        if (message == null || message.contains("BEGIN CERTIFICATE")) {
            message = "certificate provisioning failed";
        }
        return new Failure(
                "automatic trust state requires repair".equals(message)
                        ? "ProvisioningRepairRequired"
                        : "ProvisioningFailed",
                message);
    }

    private static String requireSinglePem(String value) throws Failure {
        if (value == null) throw invalidCertificate();
        byte[] bytes = value.getBytes(StandardCharsets.US_ASCII);
        if (bytes.length == 0 || bytes.length > MAX_CERTIFICATE_BYTES
                || value.indexOf('\0') >= 0) {
            throw invalidCertificate();
        }
        String begin = "-----BEGIN CERTIFICATE-----";
        String end = "-----END CERTIFICATE-----";
        int beginAt = value.indexOf(begin);
        int endAt = value.indexOf(end);
        if (beginAt < 0 || endAt <= beginAt
                || value.indexOf(begin, beginAt + begin.length()) >= 0
                || value.indexOf(end, endAt + end.length()) >= 0
                || !value.substring(0, beginAt).trim().isEmpty()
                || !value.substring(endAt + end.length()).trim().isEmpty()) {
            throw invalidCertificate();
        }
        return value;
    }

    private static X509Certificate parseSingleCertificate(String value) throws Failure {
        String pem = requireSinglePem(value);
        try {
            CertificateFactory factory = CertificateFactory.getInstance("X.509");
            ByteArrayInputStream input = new ByteArrayInputStream(
                    pem.getBytes(StandardCharsets.US_ASCII));
            X509Certificate certificate =
                    (X509Certificate) factory.generateCertificate(input);
            if (input.available() != 0) throw invalidCertificate();
            return certificate;
        } catch (Failure failure) {
            throw failure;
        } catch (Exception error) {
            throw invalidCertificate();
        }
    }

    private static Failure invalidCertificate() {
        return new Failure(
                "InvalidCertificate", "host returned an invalid server certificate");
    }
}
