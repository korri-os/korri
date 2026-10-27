package com.limelight.computers;

import com.limelight.nvstream.http.ComputerDetails;

import org.junit.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

public class KorriMoonlightComputerConcurrencyTest {
    @Test
    public void retirementAndInvalidationCannotDeadlockSnapshot() throws Exception {
        ComputerDetails details = new ComputerDetails();
        details.uuid = "host-uuid";
        details.state = ComputerDetails.State.ONLINE;
        PollingTuple tuple = new PollingTuple(details, null);
        CountDownLatch networkHeld = new CountDownLatch(1);
        CountDownLatch releaseNetwork = new CountDownLatch(1);
        AtomicReference<Throwable> failure = new AtomicReference<>();

        Thread snapshot = new Thread(() -> {
            try {
                synchronized (tuple.networkLock) {
                    networkHeld.countDown();
                    releaseNetwork.await(2, TimeUnit.SECONDS);
                    if (!tuple.retired) new ComputerDetails(tuple.computer);
                }
            } catch (Throwable error) {
                failure.set(error);
            }
        });
        Thread invalidate = new Thread(() -> {
            try {
                assertTrue(networkHeld.await(2, TimeUnit.SECONDS));
                tuple.retire();
                tuple.invalidateState();
            } catch (Throwable error) {
                failure.set(error);
            }
        });

        snapshot.start();
        invalidate.start();
        assertTrue(networkHeld.await(2, TimeUnit.SECONDS));
        releaseNetwork.countDown();
        snapshot.join(2000);
        invalidate.join(2000);

        assertFalse(snapshot.isAlive());
        assertFalse(invalidate.isAlive());
        assertNull(failure.get());
        assertNull(tuple.snapshotDetails());
    }

    @Test
    public void slowNetworkFetchDoesNotHoldHostMutationLock() throws Exception {
        Object mutationLock = new Object();
        CountDownLatch fetchEntered = new CountDownLatch(1);
        CountDownLatch releaseFetch = new CountDownLatch(1);
        CountDownLatch bridgeReadLock = new CountDownLatch(1);
        AtomicReference<Throwable> failure = new AtomicReference<>();

        Thread poller = new Thread(() -> {
            try {
                KorriMoonlightNetworkCycle.fetchThenCommit(
                        mutationLock,
                        () -> {
                            fetchEntered.countDown();
                            assertTrue(releaseFetch.await(2, TimeUnit.SECONDS));
                            return "apps";
                        },
                        value -> value.length());
            } catch (Throwable error) {
                failure.set(error);
            }
        });
        poller.start();
        assertTrue(fetchEntered.await(2, TimeUnit.SECONDS));

        Thread bridgeRead = new Thread(() -> {
            synchronized (mutationLock) {
                bridgeReadLock.countDown();
            }
        });
        bridgeRead.start();
        assertTrue(bridgeReadLock.await(200, TimeUnit.MILLISECONDS));

        releaseFetch.countDown();
        poller.join(2000);
        bridgeRead.join(2000);
        assertNull(failure.get());
    }
}
