package com.limelight.computers;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.StringReader;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.UnknownHostException;
import java.security.cert.X509Certificate;
import java.util.HashSet;
import java.util.LinkedList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

import com.limelight.LimeLog;
import com.limelight.binding.PlatformBinding;
import com.limelight.discovery.DiscoveryService;
import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.nvstream.http.HostCertificateState;
import com.limelight.nvstream.mdns.MdnsComputer;
import com.limelight.nvstream.mdns.MdnsDiscoveryListener;
import com.limelight.utils.CacheHelper;
import com.limelight.utils.NetHelper;
import com.limelight.utils.ServerHelper;

import android.app.Service;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.Binder;
import android.os.Build;
import android.os.IBinder;
import android.os.SystemClock;
import android.util.AtomicFile;

import org.xmlpull.v1.XmlPullParserException;

public class ComputerManagerService extends Service {
    public interface MoonlightCommitGuard {
        boolean current();
    }

    private static final int SERVERINFO_POLLING_PERIOD_MS = 1500;
    private static final int APPLIST_POLLING_PERIOD_MS = 30000;
    private static final int APPLIST_FAILED_POLLING_RETRY_MS = 2000;
    private static final int MDNS_QUERY_PERIOD_MS = 1000;
    private static final int OFFLINE_POLL_TRIES = 3;
    private static final int INITIAL_POLL_TRIES = 2;
    private static final int EMPTY_LIST_THRESHOLD = 3;
    private static final int POLL_DATA_TTL_MS = 30000;

    private final ComputerManagerBinder binder = new ComputerManagerBinder();

    private ComputerDatabaseManager dbManager;
    private final AtomicInteger dbRefCount = new AtomicInteger(0);

    private IdentityManager idManager;
    private final LinkedList<PollingTuple> pollingTuples = new LinkedList<>();
    private ComputerManagerListener listener = null;
    private final AtomicInteger activePolls = new AtomicInteger(0);
    private boolean pollingActive = false;
    private final Lock defaultNetworkLock = new ReentrantLock();

    private ConnectivityManager.NetworkCallback networkCallback;

    private DiscoveryService.DiscoveryBinder discoveryBinder;
    private final ServiceConnection discoveryServiceConnection = new ServiceConnection() {
        public void onServiceConnected(ComponentName className, IBinder binder) {
            synchronized (discoveryServiceConnection) {
                DiscoveryService.DiscoveryBinder privateBinder = ((DiscoveryService.DiscoveryBinder)binder);

                // Set us as the event listener
                privateBinder.setListener(createDiscoveryListener());

                // Signal a possible waiter that we're all setup
                discoveryBinder = privateBinder;
                discoveryServiceConnection.notifyAll();
            }
        }

        public void onServiceDisconnected(ComponentName className) {
            discoveryBinder = null;
        }
    };

    // Returns true if the details object was modified
    private boolean runPoll(ComputerDetails details, boolean newPc, int offlineCount) throws InterruptedException {
        if (!getLocalDatabaseReference()) {
            return false;
        }

        final int pollTriesBeforeOffline = details.state == ComputerDetails.State.UNKNOWN ?
                INITIAL_POLL_TRIES : OFFLINE_POLL_TRIES;

        activePolls.incrementAndGet();

        // Poll the machine
        try {
            if (!pollComputer(details)) {
                if (!newPc && offlineCount < pollTriesBeforeOffline) {
                    // Return without calling the listener
                    releaseLocalDatabaseReference();
                    return false;
                }

                details.state = ComputerDetails.State.OFFLINE;
            }
        } catch (InterruptedException e) {
            releaseLocalDatabaseReference();
            throw e;
        } finally {
            activePolls.decrementAndGet();
        }

        // If it's online, update our persistent state
        if (details.state == ComputerDetails.State.ONLINE) {
            ComputerDetails existingComputer = dbManager.getComputerByUUID(details.uuid);

            // Check if it's in the database because it could have been
            // removed after this was issued
            if (!newPc && existingComputer == null) {
                // It's gone
                releaseLocalDatabaseReference();
                return false;
            }

            // If we already have an entry for this computer in the DB, we must
            // combine the existing data with this new data (which may be partially available
            // due to detecting the PC via mDNS) without the saved external address. If we
            // write to the DB without doing this first, we can overwrite our existing data.
            if (existingComputer != null) {
                existingComputer.update(details);
                dbManager.updateComputer(existingComputer);
            }
            else {
                try {
                    // If the active address is a site-local address (RFC 1918),
                    // then use STUN to populate the external address field if
                    // it's not set already.
                    if (details.remoteAddress == null) {
                        InetAddress addr = InetAddress.getByName(details.activeAddress.address);
                        if (addr.isSiteLocalAddress()) {
                            populateExternalAddress(details);
                        }
                    }
                } catch (UnknownHostException ignored) {}

                dbManager.updateComputer(details);
            }
        }

        // Don't call the listener if this is a failed lookup of a new PC
        if ((!newPc || details.state == ComputerDetails.State.ONLINE) && listener != null) {
            listener.notifyComputerUpdated(details);
        }

        releaseLocalDatabaseReference();
        return true;
    }

    private Thread createPollingThread(final PollingTuple tuple) {
        Thread t = new Thread() {
            @Override
            public void run() {

                int offlineCount = 0;
                while (!isInterrupted() && pollingActive && tuple.thread == this) {
                    try {
                        // Only allow one request to the machine at a time
                        synchronized (tuple.networkLock) {
                            // Check if this poll has modified the details
                            if (!runPoll(tuple.computer, false, offlineCount)) {
                                LimeLog.warning(tuple.computer.name + " is offline (try " + offlineCount + ")");
                                offlineCount++;
                            } else {
                                tuple.lastSuccessfulPollMs = SystemClock.elapsedRealtime();
                                offlineCount = 0;
                            }
                        }

                        // Wait until the next polling interval
                        Thread.sleep(SERVERINFO_POLLING_PERIOD_MS);
                    } catch (InterruptedException e) {
                        break;
                    }
                }
            }
        };
        t.setName("Polling thread for " + tuple.computer.name);
        return t;
    }

    public class ComputerManagerBinder extends Binder {
        public void startPolling(ComputerManagerListener listener) {
            // Polling is active
            pollingActive = true;

            // Set the listener
            ComputerManagerService.this.listener = listener;

            // Start mDNS autodiscovery too
            discoveryBinder.startDiscovery(MDNS_QUERY_PERIOD_MS);

            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    // Enforce the poll data TTL
                    if (SystemClock.elapsedRealtime() - tuple.lastSuccessfulPollMs > POLL_DATA_TTL_MS) {
                        LimeLog.info("Timing out polled state for "+tuple.computer.name);
                        tuple.computer.state = ComputerDetails.State.UNKNOWN;
                    }

                    // Report this computer initially
                    listener.notifyComputerUpdated(tuple.computer);

                    // This polling thread might already be there
                    if (tuple.thread == null) {
                        tuple.thread = createPollingThread(tuple);
                        tuple.thread.start();
                    }
                }
            }
        }

        public void waitForReady() {
            synchronized (discoveryServiceConnection) {
                try {
                    while (discoveryBinder == null) {
                        // Wait for the bind notification
                        discoveryServiceConnection.wait(1000);
                    }
                } catch (InterruptedException e) {
                    e.printStackTrace();

                    // InterruptedException clears the thread's interrupt status. Since we can't
                    // handle that here, we will re-interrupt the thread to set the interrupt
                    // status back to true.
                    Thread.currentThread().interrupt();
                }
            }
        }

        public void waitForPollingStopped() {
            while (activePolls.get() != 0) {
                try {
                    Thread.sleep(250);
                } catch (InterruptedException e) {
                    e.printStackTrace();

                    // InterruptedException clears the thread's interrupt status. Since we can't
                    // handle that here, we will re-interrupt the thread to set the interrupt
                    // status back to true.
                    Thread.currentThread().interrupt();
                }
            }
        }

        public boolean addComputerBlocking(ComputerDetails fakeDetails) throws InterruptedException {
            return ComputerManagerService.this.addComputerBlocking(fakeDetails);
        }

        public boolean hasComputerAtAddress(ComputerDetails.AddressTuple address) {
            return ComputerManagerService.this.hasComputerAtAddress(address);
        }

        public boolean prepareComputerBlocking(ComputerDetails details) throws InterruptedException {
            return ComputerManagerService.this.prepareComputerBlocking(details);
        }

        public boolean commitPreparedComputer(ComputerDetails details) {
            return ComputerManagerService.this.commitPreparedComputer(details);
        }

        public void removeComputer(ComputerDetails computer) {
            ComputerManagerService.this.removeComputer(computer);
        }

        public void stopPolling() {
            // Just call the unbind handler to cleanup
            ComputerManagerService.this.onUnbind(null);
        }

        public ApplistPoller createAppListPoller(ComputerDetails computer) {
            return new ApplistPoller(computer);
        }

        public String getUniqueId() {
            return idManager.getUniqueId();
        }

        public ComputerDetails getComputer(String uuid) {
            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    if (uuid.equals(tuple.computer.uuid)) {
                        return tuple.computer;
                    }
                }
            }

            return null;
        }

        public final class MoonlightHostSnapshot {
            public final ComputerDetails computer;
            public final long generation;

            private MoonlightHostSnapshot(ComputerDetails computer, long generation) {
                this.computer = computer;
                this.generation = generation;
            }
        }

        public final class MoonlightHostCommit {
            public final boolean committed;
            public final boolean stale;
            public final ComputerDetails computer;

            private MoonlightHostCommit(
                    boolean committed, boolean stale, ComputerDetails computer) {
                this.committed = committed;
                this.stale = stale;
                this.computer = computer;
            }
        }

        /** Snapshot the exact live record and its app-list generation. */
        public MoonlightHostSnapshot snapshotMoonlightHost(String uuid) {
            PollingTuple tuple = findPollingTuple(uuid);
            if (tuple == null) return null;
            synchronized (tuple.networkLock) {
                if (tuple.retired) return null;
                return new MoonlightHostSnapshot(
                        new ComputerDetails(tuple.computer), tuple.appListGeneration);
            }
        }

        /**
         * Atomically publish a provisioned pin and app list if no newer poll won.
         * The cache and database rollback remain under the same per-host lock.
         */
        public MoonlightHostCommit commitMoonlightHost(
                String uuid,
                long expectedGeneration,
                X509Certificate serverCertificate,
                String rawAppList,
                MoonlightCommitGuard guard) throws IOException {
            PollingTuple tuple = findPollingTuple(uuid);
            if (tuple == null) return new MoonlightHostCommit(false, false, null);
            synchronized (tuple.networkLock) {
                if (tuple.retired) {
                    return new MoonlightHostCommit(false, false, null);
                }
                if (tuple.appListGeneration != expectedGeneration) {
                    return new MoonlightHostCommit(
                            false, true, new ComputerDetails(tuple.computer));
                }
                if (!guard.current()) {
                    return new MoonlightHostCommit(false, false, null);
                }
                if (!getLocalDatabaseReference()) {
                    return new MoonlightHostCommit(false, false, null);
                }
                byte[] previousCache = readAppListCache(uuid);
                X509Certificate previousCertificate = tuple.computer.serverCert;
                String previousRawAppList = tuple.computer.rawAppList;
                try {
                    writeAppListCache(uuid, rawAppList);
                    tuple.computer.serverCert = serverCertificate;
                    tuple.computer.rawAppList = rawAppList;
                    if (!dbManager.updateComputer(tuple.computer)) {
                        throw new IOException("computer disappeared during provisioning");
                    }
                    tuple.appListGeneration++;
                    return new MoonlightHostCommit(true, false, tuple.computer);
                } catch (IOException | RuntimeException error) {
                    tuple.computer.serverCert = previousCertificate;
                    tuple.computer.rawAppList = previousRawAppList;
                    try {
                        writeAppListCache(uuid, previousCache);
                        if (!dbManager.updateComputer(tuple.computer)) {
                            throw new IOException("could not restore computer after provisioning");
                        }
                    } catch (IOException | RuntimeException repairFailure) {
                        IOException fatal = new IOException(
                                "automatic trust state requires repair", error);
                        fatal.addSuppressed(repairFailure);
                        throw fatal;
                    }
                    if (error instanceof IOException) throw (IOException) error;
                    throw error;
                } finally {
                    releaseLocalDatabaseReference();
                }
            }
        }

        private PollingTuple findPollingTuple(String uuid) {
            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    if (uuid.equals(tuple.computer.uuid) && !tuple.retired) return tuple;
                }
            }
            return null;
        }

        /** Persist an exact server pin without allowing a concurrent poll to overwrite it. */
        public boolean updateServerCertificate(String uuid, X509Certificate serverCertificate) {
            PollingTuple tuple = findPollingTuple(uuid);
            if (tuple == null) return false;
            synchronized (tuple.networkLock) {
                if (tuple.retired || !getLocalDatabaseReference()) return false;
                X509Certificate previous = tuple.computer.serverCert;
                try {
                    tuple.computer.serverCert = serverCertificate;
                    if (!dbManager.updateComputer(tuple.computer)) {
                        tuple.computer.serverCert = previous;
                        return false;
                    }
                    return true;
                } finally {
                    releaseLocalDatabaseReference();
                }
            }
        }

        public void invalidateStateForComputer(String uuid) {
            PollingTuple tuple = findPollingTuple(uuid);
            if (tuple == null) return;
            // Lock order is registry lookup, release, then tuple mutation. Code
            // holding networkLock never reacquires pollingTuples.
            tuple.invalidateState();
        }
    }

    private byte[] readAppListCache(String uuid) throws IOException {
        File target = CacheHelper.openPath(false, getCacheDir(), "applist", uuid);
        if (!target.exists()) return null;
        try (InputStream input = new FileInputStream(target);
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096];
            int count;
            while ((count = input.read(buffer)) != -1) {
                output.write(buffer, 0, count);
            }
            return output.toByteArray();
        }
    }

    private void writeAppListCache(String uuid, String rawAppList) throws IOException {
        writeAppListCache(
                uuid,
                rawAppList == null
                        ? null
                        : rawAppList.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    private void writeAppListCache(String uuid, byte[] rawAppList) throws IOException {
        File target = CacheHelper.openPath(rawAppList != null, getCacheDir(), "applist", uuid);
        if (rawAppList == null) {
            if (target.exists() && !target.delete()) {
                throw new IOException("could not restore app-list cache");
            }
            return;
        }
        AtomicFile atomicFile = new AtomicFile(target);
        FileOutputStream output = null;
        try {
            output = atomicFile.startWrite();
            output.write(rawAppList);
            atomicFile.finishWrite(output);
        } catch (IOException | RuntimeException error) {
            if (output != null) atomicFile.failWrite(output);
            throw error;
        }
    }

    @Override
    public boolean onUnbind(Intent intent) {
        if (discoveryBinder != null) {
            // Stop mDNS autodiscovery
            discoveryBinder.stopDiscovery();
        }

        // Stop polling
        pollingActive = false;
        synchronized (pollingTuples) {
            for (PollingTuple tuple : pollingTuples) {
                if (tuple.thread != null) {
                    // Interrupt and remove the thread
                    tuple.thread.interrupt();
                    tuple.thread = null;
                }
            }
        }

        // Remove the listener
        listener = null;

        return false;
    }

    private void populateExternalAddress(ComputerDetails details) {
        boolean boundToNetwork = false;
        boolean activeNetworkIsVpn = NetHelper.isActiveNetworkVpn(this);
        ConnectivityManager connMgr = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);

        // Check if we're currently connected to a VPN which may send our
        // STUN request from an unexpected interface
        if (activeNetworkIsVpn) {
            // Acquire the default network lock since we could be changing global process state
            defaultNetworkLock.lock();

            // On Lollipop or later, we can bind our process to the underlying interface
            // to ensure our STUN request goes out on that interface or not at all (which is
            // preferable to getting a VPN endpoint address back).
            Network[] networks = connMgr.getAllNetworks();
            for (Network net : networks) {
                NetworkCapabilities netCaps = connMgr.getNetworkCapabilities(net);
                if (netCaps != null) {
                    if (!netCaps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) &&
                            !netCaps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) {
                        // This network looks like an underlying multicast-capable transport,
                        // so let's guess that it's probably where our mDNS response came from.
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                            if (connMgr.bindProcessToNetwork(net)) {
                                boundToNetwork = true;
                                break;
                            }
                        } else if (ConnectivityManager.setProcessDefaultNetwork(net)) {
                            boundToNetwork = true;
                            break;
                        }
                    }
                }
            }

            // Perform the STUN request if we're not on a VPN or if we bound to a network
            if (!activeNetworkIsVpn || boundToNetwork) {
                String stunResolvedAddress = NvConnection.findExternalAddressForMdns("stun.moonlight-stream.org", 3478);
                if (stunResolvedAddress != null) {
                    // We don't know for sure what the external port is, so we will have to guess.
                    // When we contact the PC (if we haven't already), it will update the port.
                    details.remoteAddress = new ComputerDetails.AddressTuple(stunResolvedAddress, details.guessExternalPort());
                }
            }

            // Unbind from the network
            if (boundToNetwork) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                    connMgr.bindProcessToNetwork(null);
                } else {
                    ConnectivityManager.setProcessDefaultNetwork(null);
                }
            }

            // Unlock the network state
            if (activeNetworkIsVpn) {
                defaultNetworkLock.unlock();
            }
        }
    }

    private MdnsDiscoveryListener createDiscoveryListener() {
        return new MdnsDiscoveryListener() {
            @Override
            public void notifyComputerAdded(MdnsComputer computer) {
                ComputerDetails details = new ComputerDetails();

                // Populate the computer template with mDNS info
                if (computer.getLocalAddress() != null) {
                    details.localAddress = new ComputerDetails.AddressTuple(computer.getLocalAddress().getHostAddress(), computer.getPort());

                    // Since we're on the same network, we can use STUN to find
                    // our WAN address, which is also very likely the WAN address
                    // of the PC. We can use this later to connect remotely.
                    if (computer.getLocalAddress() instanceof Inet4Address) {
                        populateExternalAddress(details);
                    }
                }
                if (computer.getIpv6Address() != null) {
                    details.ipv6Address = new ComputerDetails.AddressTuple(computer.getIpv6Address().getHostAddress(), computer.getPort());
                }

                try {
                    // Kick off a blocking serverinfo poll on this machine
                    if (!addComputerBlocking(details)) {
                        LimeLog.warning("Auto-discovered PC failed to respond: "+details);
                    }
                } catch (InterruptedException e) {
                    e.printStackTrace();

                    // InterruptedException clears the thread's interrupt status. Since we can't
                    // handle that here, we will re-interrupt the thread to set the interrupt
                    // status back to true.
                    Thread.currentThread().interrupt();
                }
            }

            @Override
            public void notifyDiscoveryFailure(Exception e) {
                LimeLog.severe("mDNS discovery failed");
                e.printStackTrace();
            }
        };
    }

    private void addTuple(ComputerDetails details) {
        synchronized (pollingTuples) {
            for (PollingTuple tuple : pollingTuples) {
                // Check if this is the same computer
                if (tuple.computer.uuid.equals(details.uuid)) {
                    // Update the saved computer with potentially new details
                    tuple.computer.update(details);

                    // Start a polling thread if polling is active
                    if (pollingActive && tuple.thread == null) {
                        tuple.thread = createPollingThread(tuple);
                        tuple.thread.start();
                    }

                    // Found an entry so we're done
                    return;
                }
            }

            // If we got here, we didn't find an entry
            PollingTuple tuple = new PollingTuple(details, null);
            if (pollingActive) {
                tuple.thread = createPollingThread(tuple);
            }
            pollingTuples.add(tuple);
            if (tuple.thread != null) {
                tuple.thread.start();
            }
        }
    }

    private boolean hasComputerAtAddress(ComputerDetails.AddressTuple address) {
        if (address == null) return false;
        List<PollingTuple> snapshot;
        synchronized (pollingTuples) {
            snapshot = new LinkedList<>(pollingTuples);
        }
        for (PollingTuple tuple : snapshot) {
            synchronized (tuple.networkLock) {
                if (!tuple.retired && address.equals(tuple.computer.manualAddress)) return true;
            }
        }
        return false;
    }

    /** Run all network work needed to add a host without mutating persistent state. */
    private boolean prepareComputerBlocking(ComputerDetails details) throws InterruptedException {
        activePolls.incrementAndGet();
        try {
            if (!pollComputer(details)) return false;

            PollingTuple existing = null;
            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    if (details.uuid.equals(tuple.computer.uuid) && !tuple.retired) {
                        existing = tuple;
                        break;
                    }
                }
            }
            if (existing != null) {
                synchronized (existing.networkLock) {
                    if (!existing.retired) details.serverCert = existing.computer.serverCert;
                }
            }

            if (!pollComputer(details) || details.state != ComputerDetails.State.ONLINE) {
                return false;
            }
            try {
                if (details.remoteAddress == null) {
                    InetAddress active = InetAddress.getByName(details.activeAddress.address);
                    if (active.isSiteLocalAddress()) populateExternalAddress(details);
                }
            } catch (UnknownHostException ignored) {}
            return true;
        } finally {
            activePolls.decrementAndGet();
        }
    }

    /** Commit one fully polled host. The caller owns any lifecycle commit fence. */
    private boolean commitPreparedComputer(ComputerDetails details) {
        if (details.state != ComputerDetails.State.ONLINE || details.uuid == null) return false;

        PollingTuple existing = null;
        synchronized (pollingTuples) {
            for (PollingTuple tuple : pollingTuples) {
                if (details.uuid.equals(tuple.computer.uuid) && !tuple.retired) {
                    existing = tuple;
                    break;
                }
            }
        }
        if (existing != null) {
            synchronized (existing.networkLock) {
                if (existing.retired || !getLocalDatabaseReference()) return false;
                try {
                    X509Certificate currentCertificate = existing.computer.serverCert;
                    HostCertificateState currentCertificateState =
                            existing.computer.certificateState;
                    String currentRawAppList = existing.computer.rawAppList;
                    existing.computer.update(details);
                    existing.computer.serverCert = currentCertificate;
                    existing.computer.certificateState = currentCertificateState;
                    existing.computer.rawAppList = currentRawAppList;
                    if (!dbManager.updateComputer(existing.computer)) return false;
                    if (listener != null) listener.notifyComputerUpdated(existing.computer);
                    return true;
                } finally {
                    releaseLocalDatabaseReference();
                }
            }
        }

        if (!getLocalDatabaseReference()) return false;
        try {
            if (!dbManager.updateComputer(details)) return false;
            if (listener != null) listener.notifyComputerUpdated(details);
            addTuple(details);
            return true;
        } finally {
            releaseLocalDatabaseReference();
        }
    }

    public boolean addComputerBlocking(ComputerDetails fakeDetails) throws InterruptedException {
        // Block while we try to fill the details

        // We cannot use runPoll() here because it will attempt to persist the state of the machine
        // in the database, which would be bad because we don't have our pinned cert loaded yet.
        if (pollComputer(fakeDetails)) {
            // See if we have record of this PC to pull its pinned cert
            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    if (tuple.computer.uuid.equals(fakeDetails.uuid)) {
                        fakeDetails.serverCert = tuple.computer.serverCert;
                        break;
                    }
                }
            }

            // Poll again with the pinned cert to get accurate certificate acceptance state.
            // This will insert the host into the database too.
            runPoll(fakeDetails, true, 0);
        }

        // If the machine is reachable, it was successful
        if (fakeDetails.state == ComputerDetails.State.ONLINE) {
            LimeLog.info("New PC ("+fakeDetails.name+") is UUID "+fakeDetails.uuid);

            // Start a polling thread for this machine
            addTuple(fakeDetails);
            return true;
        }
        else {
            return false;
        }
    }

    public void removeComputer(ComputerDetails computer) {
        if (!getLocalDatabaseReference()) {
            return;
        }

        // Remove it from the database
        dbManager.deleteComputer(computer);

        synchronized (pollingTuples) {
            // Remove the computer from the computer list
            for (PollingTuple tuple : pollingTuples) {
                if (tuple.computer.uuid.equals(computer.uuid)) {
                    if (tuple.thread != null) {
                        // Interrupt the thread on this entry
                        tuple.thread.interrupt();
                        tuple.thread = null;
                    }
                    // Retirement is volatile, so a caller that already holds
                    // this tuple can reject it without reacquiring pollingTuples.
                    tuple.retire();
                    pollingTuples.remove(tuple);
                    break;
                }
            }
        }

        releaseLocalDatabaseReference();
    }

    private boolean getLocalDatabaseReference() {
        if (dbRefCount.get() == 0) {
            return false;
        }

        dbRefCount.incrementAndGet();
        return true;
    }

    private void releaseLocalDatabaseReference() {
        if (dbRefCount.decrementAndGet() == 0) {
            dbManager.close();
        }
    }

    private ComputerDetails tryPollIp(ComputerDetails details, ComputerDetails.AddressTuple address) {
        try {
            // If the current address's port number matches the active address's port number, we can also assume
            // the HTTPS port will also match. This assumption is currently safe because Sunshine sets all ports
            // as offsets from the base HTTP port and doesn't allow custom HttpsPort responses for WAN vs LAN.
            boolean portMatchesActiveAddress = details.state == ComputerDetails.State.ONLINE &&
                    details.activeAddress != null && address.port == details.activeAddress.port;

            NvHTTP http = new NvHTTP(address, portMatchesActiveAddress ? details.httpsPort : 0, idManager.getUniqueId(), details.serverCert,
                    PlatformBinding.getCryptoProvider(ComputerManagerService.this));

            // If this PC is currently online at this address, extend the timeouts to allow more time for the PC to respond.
            boolean isLikelyOnline = details.state == ComputerDetails.State.ONLINE && address.equals(details.activeAddress);

            ComputerDetails newDetails = http.getComputerDetails(isLikelyOnline);

            // Check if this is the PC we expected
            if (newDetails.uuid == null) {
                LimeLog.severe("Polling returned no UUID!");
                return null;
            }
            // details.uuid can be null on initial PC add
            else if (details.uuid != null && !details.uuid.equals(newDetails.uuid)) {
                // We got the wrong PC!
                LimeLog.info("Polling returned the wrong PC!");
                return null;
            }

            return newDetails;
        } catch (XmlPullParserException e) {
            e.printStackTrace();
            return null;
        } catch (IOException e) {
            return null;
        }
    }

    private static class ParallelPollTuple {
        public ComputerDetails.AddressTuple address;
        public ComputerDetails existingDetails;

        public boolean complete;
        public Thread pollingThread;
        public ComputerDetails returnedDetails;

        public ParallelPollTuple(ComputerDetails.AddressTuple address, ComputerDetails existingDetails) {
            this.address = address;
            this.existingDetails = existingDetails;
        }

        public void interrupt() {
            if (pollingThread != null) {
                pollingThread.interrupt();
            }
        }
    }

    private void startParallelPollThread(ParallelPollTuple tuple, HashSet<ComputerDetails.AddressTuple> uniqueAddresses) {
        // Don't bother starting a polling thread for an address that doesn't exist
        // or if the address has already been polled with an earlier tuple
        if (tuple.address == null || !uniqueAddresses.add(tuple.address)) {
            tuple.complete = true;
            tuple.returnedDetails = null;
            return;
        }

        tuple.pollingThread = new Thread() {
            @Override
            public void run() {
                ComputerDetails details = tryPollIp(tuple.existingDetails, tuple.address);

                synchronized (tuple) {
                    tuple.complete = true; // Done
                    tuple.returnedDetails = details; // Polling result

                    tuple.notify();
                }
            }
        };
        tuple.pollingThread.setName("Parallel Poll - "+tuple.address+" - "+tuple.existingDetails.name);
        tuple.pollingThread.start();
    }

    private ComputerDetails parallelPollPc(ComputerDetails details) throws InterruptedException {
        ParallelPollTuple localInfo = new ParallelPollTuple(details.localAddress, details);
        ParallelPollTuple manualInfo = new ParallelPollTuple(details.manualAddress, details);
        ParallelPollTuple remoteInfo = new ParallelPollTuple(details.remoteAddress, details);
        ParallelPollTuple ipv6Info = new ParallelPollTuple(details.ipv6Address, details);

        // These must be started in order of precedence for the deduplication algorithm
        // to result in the correct behavior.
        HashSet<ComputerDetails.AddressTuple> uniqueAddresses = new HashSet<>();
        startParallelPollThread(localInfo, uniqueAddresses);
        startParallelPollThread(manualInfo, uniqueAddresses);
        startParallelPollThread(remoteInfo, uniqueAddresses);
        startParallelPollThread(ipv6Info, uniqueAddresses);

        try {
            // Check local first
            synchronized (localInfo) {
                while (!localInfo.complete) {
                    localInfo.wait();
                }

                if (localInfo.returnedDetails != null) {
                    localInfo.returnedDetails.activeAddress = localInfo.address;
                    return localInfo.returnedDetails;
                }
            }

            // Now manual
            synchronized (manualInfo) {
                while (!manualInfo.complete) {
                    manualInfo.wait();
                }

                if (manualInfo.returnedDetails != null) {
                    manualInfo.returnedDetails.activeAddress = manualInfo.address;
                    return manualInfo.returnedDetails;
                }
            }

            // Now remote IPv4
            synchronized (remoteInfo) {
                while (!remoteInfo.complete) {
                    remoteInfo.wait();
                }

                if (remoteInfo.returnedDetails != null) {
                    remoteInfo.returnedDetails.activeAddress = remoteInfo.address;
                    return remoteInfo.returnedDetails;
                }
            }

            // Now global IPv6
            synchronized (ipv6Info) {
                while (!ipv6Info.complete) {
                    ipv6Info.wait();
                }

                if (ipv6Info.returnedDetails != null) {
                    ipv6Info.returnedDetails.activeAddress = ipv6Info.address;
                    return ipv6Info.returnedDetails;
                }
            }
        } finally {
            // Stop any further polling if we've found a working address or we've been
            // interrupted by an attempt to stop polling.
            localInfo.interrupt();
            manualInfo.interrupt();
            remoteInfo.interrupt();
            ipv6Info.interrupt();
        }

        return null;
    }

    private boolean pollComputer(ComputerDetails details) throws InterruptedException {
        // Poll all addresses in parallel to speed up the process
        LimeLog.info("Starting parallel poll for "+details.name+" ("+details.localAddress +", "+details.remoteAddress +", "+details.manualAddress+", "+details.ipv6Address+")");
        ComputerDetails polledDetails = parallelPollPc(details);
        LimeLog.info("Parallel poll for "+details.name+" returned address: "+details.activeAddress);

        if (polledDetails != null) {
            details.update(polledDetails);
            return true;
        }
        else {
            return false;
        }
    }

    @Override
    public void onCreate() {
        // Bind to the discovery service
        bindService(new Intent(this, DiscoveryService.class),
                discoveryServiceConnection, Service.BIND_AUTO_CREATE);

        // Lookup or generate this device's UID
        idManager = new IdentityManager(this);

        // Initialize the DB
        dbManager = new ComputerDatabaseManager(this);
        dbRefCount.set(1);

        // Grab known machines into our computer list
        if (!getLocalDatabaseReference()) {
            return;
        }

        for (ComputerDetails computer : dbManager.getAllComputers()) {
            // Add tuples for each computer
            addTuple(computer);
        }

        releaseLocalDatabaseReference();

        // Monitor for network changes to invalidate our PC state
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            networkCallback = new ConnectivityManager.NetworkCallback() {
                @Override
                public void onAvailable(Network network) {
                    LimeLog.info("Resetting PC state for new available network");
                    synchronized (pollingTuples) {
                        for (PollingTuple tuple : pollingTuples) {
                            tuple.computer.state = ComputerDetails.State.UNKNOWN;
                            if (listener != null) {
                                listener.notifyComputerUpdated(tuple.computer);
                            }
                        }
                    }
                }

                @Override
                public void onLost(Network network) {
                    LimeLog.info("Offlining PCs due to network loss");
                    synchronized (pollingTuples) {
                        for (PollingTuple tuple : pollingTuples) {
                            tuple.computer.state = ComputerDetails.State.OFFLINE;
                            if (listener != null) {
                                listener.notifyComputerUpdated(tuple.computer);
                            }
                        }
                    }
                }
            };

            ConnectivityManager connMgr = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
            connMgr.registerDefaultNetworkCallback(networkCallback);
        }
    }

    @Override
    public void onDestroy() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            ConnectivityManager connMgr = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
            connMgr.unregisterNetworkCallback(networkCallback);
        }

        if (discoveryBinder != null) {
            // Unbind from the discovery service
            unbindService(discoveryServiceConnection);
        }

        // FIXME: Should await termination here but we have timeout issues in HttpURLConnection

        // Remove the initial DB reference
        releaseLocalDatabaseReference();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    public class ApplistPoller {
        private Thread thread;
        private final ComputerDetails computer;
        private final Object pollEvent = new Object();
        private boolean receivedAppList = false;

        public ApplistPoller(ComputerDetails computer) {
            this.computer = computer;
        }

        public void pollNow() {
            synchronized (pollEvent) {
                pollEvent.notify();
            }
        }

        private boolean waitPollingDelay() {
            try {
                synchronized (pollEvent) {
                    if (receivedAppList) {
                        // If we've already reported an app list successfully,
                        // wait the full polling period
                        pollEvent.wait(APPLIST_POLLING_PERIOD_MS);
                    }
                    else {
                        // If we've failed to get an app list so far, retry much earlier
                        pollEvent.wait(APPLIST_FAILED_POLLING_RETRY_MS);
                    }
                }
            } catch (InterruptedException e) {
                return false;
            }

            return thread != null && !thread.isInterrupted();
        }

        private PollingTuple getPollingTuple(ComputerDetails details) {
            synchronized (pollingTuples) {
                for (PollingTuple tuple : pollingTuples) {
                    if (details.uuid.equals(tuple.computer.uuid)) {
                        return tuple;
                    }
                }
            }

            return null;
        }

        public void start() {
            thread = new Thread() {
                @Override
                public void run() {
                    int emptyAppListResponses = 0;
                    do {
                        // Poll only after the host accepts this device certificate.
                        if (computer.state != ComputerDetails.State.ONLINE ||
                                computer.certificateState != HostCertificateState.ACCEPTED) {
                            if (listener != null) {
                                listener.notifyComputerUpdated(computer);
                            }
                            continue;
                        }

                        // Can't poll if there's no UUID yet
                        if (computer.uuid == null) {
                            continue;
                        }

                        PollingTuple tuple = getPollingTuple(computer);

                        try {
                            PollOutcome outcome = pollAppList(
                                    tuple, emptyAppListResponses);
                            emptyAppListResponses = outcome.emptyResponses;
                            if (outcome.committed && listener != null && thread != null) {
                                listener.notifyComputerUpdated(computer);
                            }
                        } catch (Exception e) {
                            e.printStackTrace();
                        }
                    } while (waitPollingDelay());
                }
            };
            thread.setName("App list polling thread for " + computer.name);
            thread.start();
        }

        private final class FetchedAppList {
            final String raw;
            final List<NvApp> apps;
            final int emptyResponses;

            FetchedAppList(String raw, List<NvApp> apps, int emptyResponses) {
                this.raw = raw;
                this.apps = apps;
                this.emptyResponses = emptyResponses;
            }
        }

        private final class PollOutcome {
            final int emptyResponses;
            final boolean committed;

            PollOutcome(int emptyResponses, boolean committed) {
                this.emptyResponses = emptyResponses;
                this.committed = committed;
            }
        }

        private PollOutcome pollAppList(PollingTuple tuple, int emptyResponses)
                throws Exception {
            if (tuple == null) {
                ComputerDetails networkComputer = new ComputerDetails(computer);
                NvHTTP http = appListHttp(networkComputer);
                return commitFetchedAppList(fetchAppList(http, emptyResponses), null, -1);
            }
            final long expectedGeneration;
            final ComputerDetails networkComputer;
            synchronized (tuple.networkLock) {
                if (tuple.retired) return new PollOutcome(emptyResponses, false);
                expectedGeneration = tuple.appListGeneration;
                networkComputer = new ComputerDetails(tuple.computer);
            }
            NvHTTP http = appListHttp(networkComputer);
            return KorriMoonlightNetworkCycle.fetchThenCommit(
                    tuple.networkLock,
                    () -> fetchAppList(http, emptyResponses),
                    fetched -> commitFetchedAppList(fetched, tuple, expectedGeneration));
        }

        private NvHTTP appListHttp(ComputerDetails networkComputer) throws Exception {
            return new NvHTTP(
                    ServerHelper.getCurrentAddressFromComputer(networkComputer),
                    networkComputer.httpsPort,
                    idManager.getUniqueId(),
                    networkComputer.serverCert,
                    PlatformBinding.getCryptoProvider(ComputerManagerService.this));
        }

        private FetchedAppList fetchAppList(NvHTTP http, int emptyResponses)
                throws IOException, XmlPullParserException {
            String appList = http.getAppListRaw();
            List<NvApp> list = NvHTTP.getAppListByReader(new StringReader(appList));
            int nextEmptyResponses = emptyResponses;
            if (list.isEmpty()) {
                LimeLog.warning("Empty app list received from " + computer.uuid);
                nextEmptyResponses++;
            }
            return new FetchedAppList(appList, list, nextEmptyResponses);
        }

        private PollOutcome commitFetchedAppList(
                FetchedAppList fetched, PollingTuple tuple, long expectedGeneration)
                throws IOException {
            if (tuple != null
                    && (tuple.retired || tuple.appListGeneration != expectedGeneration)) {
                return new PollOutcome(fetched.emptyResponses, false);
            }
            int nextEmptyResponses = fetched.emptyResponses;
            if (!fetched.raw.isEmpty()
                    && (!fetched.apps.isEmpty()
                    || nextEmptyResponses >= EMPTY_LIST_THRESHOLD)) {
                writeAppListCache(computer.uuid, fetched.raw);
                if (!fetched.apps.isEmpty()) nextEmptyResponses = 0;
                computer.rawAppList = fetched.raw;
                receivedAppList = true;
                if (tuple != null) tuple.appListGeneration++;
                return new PollOutcome(nextEmptyResponses, true);
            }
            if (fetched.raw.isEmpty()) {
                LimeLog.warning("Null app list received from " + computer.uuid);
            }
            return new PollOutcome(nextEmptyResponses, false);
        }

        public void stop() {
            if (thread != null) {
                thread.interrupt();

                // Don't join here because we might be blocked on network I/O

                thread = null;
            }
        }
    }
}

class PollingTuple {
    public Thread thread;
    public final ComputerDetails computer;
    public final Object networkLock;
    public long lastSuccessfulPollMs;
    public long appListGeneration;
    public volatile boolean retired;

    public PollingTuple(ComputerDetails computer, Thread thread) {
        this.computer = computer;
        this.thread = thread;
        this.networkLock = new Object();
    }

    void invalidateState() {
        synchronized (networkLock) {
            if (!retired) computer.state = ComputerDetails.State.UNKNOWN;
        }
    }

    void retire() {
        retired = true;
    }

    ComputerDetails snapshotDetails() {
        synchronized (networkLock) {
            return retired ? null : new ComputerDetails(computer);
        }
    }
}

class ReachabilityTuple {
    public final String reachableAddress;
    public final ComputerDetails computer;

    public ReachabilityTuple(ComputerDetails computer, String reachableAddress) {
        this.computer = computer;
        this.reachableAddress = reachableAddress;
    }
}