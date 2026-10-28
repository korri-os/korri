package com.limelight.nvstream;

import java.util.concurrent.Semaphore;

/** Owns the startup thread and exact lease on the process-global native decoder. */
final class NvConnectionLifecycle {
    interface NativeStart { int run(); }

    interface Bridge {
        void interrupt();
        void stop();
        void cleanup();
    }

    static final class Slot {
        final Object nativeMonitor;
        final Semaphore available = new Semaphore(1);
        NvConnectionLifecycle owner;
        Slot(Object nativeMonitor) { this.nativeMonitor = nativeMonitor; }
    }

    private final Slot slot;
    private final Bridge bridge;
    private volatile boolean stopped;
    private Thread startup;

    NvConnectionLifecycle(Slot slot, Bridge bridge) {
        this.slot = slot;
        this.bridge = bridge;
    }

    boolean isStopped() { return stopped; }

    void start(Runnable work) {
        synchronized (slot) {
            if (stopped || startup != null) throw new IllegalStateException("Connection startup is retired");
            startup = new Thread(work, "moonlight-connection-start");
            startup.start();
        }
    }

    boolean connect(NativeStart startNative) throws InterruptedException {
        if (stopped) return false;
        slot.available.acquire();
        synchronized (slot) {
            if (stopped) {
                slot.available.release();
                return false;
            }
            slot.owner = this;
        }
        synchronized (slot.nativeMonitor) {
            if (stopped) {
                release();
                return false;
            }
            // Exceptions leave ownership uncertain. stop() must prove cleanup
            // before another connection can use this native bridge.
            if (startNative.run() != 0) {
                bridge.cleanup();
                release();
                return false;
            }
            return true;
        }
    }

    void stop() {
        Thread thread;
        synchronized (slot) {
            stopped = true;
            thread = startup;
            // Ownership stays held until cleanup. A late stop must never
            // interrupt a replacement connection or inflate its semaphore.
            if (slot.owner == this) bridge.interrupt();
        }
        if (thread == Thread.currentThread()) {
            throw new IllegalStateException("Connection cannot join its own startup");
        }
        if (thread != null) {
            thread.interrupt();
            try {
                thread.join();
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("Connection teardown was interrupted");
            }
        }
        synchronized (slot) {
            if (slot.owner != this) return;
        }
        synchronized (slot.nativeMonitor) {
            synchronized (slot) {
                if (slot.owner != this) return;
            }
            bridge.stop();
            bridge.cleanup();
            release();
        }
    }

    private void release() {
        synchronized (slot) {
            if (slot.owner == this) {
                slot.owner = null;
                slot.available.release();
            }
        }
    }
}
