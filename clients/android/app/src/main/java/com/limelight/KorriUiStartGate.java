package com.limelight;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

/** Atomically distinguishes a cancelled queued UI start from one already executing. */
public final class KorriUiStartGate {
    private static final int QUEUED = 0;
    private static final int STARTED = 1;
    private static final int CANCELLED = 2;

    public interface Dispatcher {
        void dispatch(Runnable callback);
    }

    public interface Start {
        void run() throws Exception;
    }

    private KorriUiStartGate() {}

    public static void run(
            Dispatcher dispatcher,
            Start start,
            long timeout,
            TimeUnit unit,
            String timeoutMessage) throws Exception {
        AtomicInteger gate = new AtomicInteger(QUEUED);
        AtomicReference<Exception> startError = new AtomicReference<>();
        CountDownLatch completed = new CountDownLatch(1);
        dispatcher.dispatch(() -> {
            if (!gate.compareAndSet(QUEUED, STARTED)) return;
            try {
                start.run();
            } catch (Exception error) {
                startError.set(error);
            } finally {
                completed.countDown();
            }
        });

        boolean interrupted = false;
        boolean finished;
        try {
            finished = completed.await(timeout, unit);
        } catch (InterruptedException error) {
            interrupted = true;
            finished = false;
        }
        if (!finished && gate.compareAndSet(QUEUED, CANCELLED)) {
            if (interrupted) {
                Thread.currentThread().interrupt();
                throw new InterruptedException();
            }
            throw new IllegalStateException(timeoutMessage);
        }
        while (!finished) {
            try {
                completed.await();
                finished = true;
            } catch (InterruptedException error) {
                interrupted = true;
            }
        }
        if (interrupted) Thread.currentThread().interrupt();
        Exception error = startError.get();
        if (error != null) throw error;
    }
}
