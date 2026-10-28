package com.limelight;

import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.TimeUnit;

/** Observed Artemis resources for one korrid-owned exact launch, not launch authority. */
final class KorriStreamViewerState {
    enum Phase { STARTING, RUNNING, BACKGROUNDING, BACKGROUNDED, STOPPING, STOPPED, FAILED }
    private enum Target { FOREGROUND, BACKGROUND, STOPPED }
    private enum Connection { WAITING, CONNECTED, CLOSED }

    final String launchId;
    final long generation;
    private Target target = Target.FOREGROUND;
    private Connection connection = Connection.WAITING;
    private final Set<Long> openConnections = new HashSet<>();
    private long nextLease;
    private long activityLease;
    private boolean focused;
    private boolean awaitingActivity = true;
    private boolean failed;

    KorriStreamViewerState(String launchId, long generation) {
        if (launchId == null || !launchId.matches("[0-9a-f]{32}") || generation <= 0) {
            throw new IllegalArgumentException("Invalid exact viewer identity");
        }
        this.launchId = launchId;
        this.generation = generation;
    }

    synchronized long attach(String launchId, long generation) {
        if (!this.launchId.equals(launchId) || this.generation != generation
                || target != Target.FOREGROUND || failed || activityLease != 0) return 0;
        activityLease = ++nextLease;
        connection = Connection.WAITING;
        focused = false;
        awaitingActivity = false;
        notifyAll();
        return activityLease;
    }

    synchronized long activityLease() {
        return activityLease;
    }

    synchronized void failedToAttach() {
        if (activityLease == 0 && awaitingActivity) {
            failed = true;
            notifyAll();
        }
    }

    synchronized boolean isCurrent(long lease) {
        return lease != 0 && activityLease == lease;
    }

    synchronized boolean canPresent(long lease) {
        return isCurrent(lease) && target == Target.FOREGROUND && !failed;
    }

    synchronized boolean opening(long lease) {
        if (!canPresent(lease)) return false;
        openConnections.add(lease);
        connection = Connection.WAITING;
        notifyAll();
        return true;
    }

    synchronized void connected(long lease) {
        if (isCurrent(lease) && openConnections.contains(lease)) {
            connection = Connection.CONNECTED;
            notifyAll();
        }
    }

    synchronized void focus(long lease, boolean hasFocus) {
        if (isCurrent(lease)) {
            focused = hasFocus;
            notifyAll();
        }
    }

    synchronized void failed(long lease) {
        if (canPresent(lease)) {
            failed = true;
            notifyAll();
        }
    }

    synchronized void closed(long lease) {
        // A retired Activity still owes its own resource-close observation.
        // Its late callback cannot close the replacement Activity's stream.
        openConnections.remove(lease);
        if (isCurrent(lease)) connection = Connection.CLOSED;
        notifyAll();
    }

    synchronized void destroyed(long lease, boolean recreating) {
        if (isCurrent(lease)) {
            activityLease = 0;
            focused = false;
            awaitingActivity = recreating && target == Target.FOREGROUND;
            notifyAll();
        }
    }

    synchronized void background() {
        target = Target.BACKGROUND;
        awaitingActivity = false;
        notifyAll();
    }

    synchronized void resume() {
        if (phase() != Phase.BACKGROUNDED || activityLease != 0 || !openConnections.isEmpty()) {
            throw new IllegalStateException("Exact viewer is not settled in the background");
        }
        target = Target.FOREGROUND;
        connection = Connection.WAITING;
        awaitingActivity = true;
        notifyAll();
    }

    synchronized void stop() {
        target = Target.STOPPED;
        awaitingActivity = false;
        notifyAll();
    }

    synchronized Phase phase() {
        boolean released = activityLease == 0 && openConnections.isEmpty();
        if (target == Target.STOPPED) return released ? Phase.STOPPED : Phase.STOPPING;
        if (target == Target.BACKGROUND) return released ? Phase.BACKGROUNDED : Phase.BACKGROUNDING;
        if (failed) return Phase.FAILED;
        if (activityLease == 0) {
            return released && !awaitingActivity ? Phase.STOPPED : Phase.STARTING;
        }
        if (connection == Connection.CONNECTED) return focused ? Phase.RUNNING : Phase.BACKGROUNDED;
        return Phase.STARTING;
    }

    synchronized boolean await(Phase expected, long timeoutMillis) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMillis);
        while (phase() != expected) {
            if (expected == Phase.RUNNING && target != Target.FOREGROUND) return false;
            if (phase() == Phase.FAILED && expected != Phase.STOPPED) return false;
            if (phase() == Phase.STOPPED && expected == Phase.RUNNING) return false;
            long remaining = deadline - System.nanoTime();
            if (remaining <= 0) return false;
            TimeUnit.NANOSECONDS.timedWait(this, remaining);
        }
        return true;
    }
}
