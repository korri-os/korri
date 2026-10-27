package com.limelight.korri.moonlight;

import java.util.concurrent.locks.ReentrantReadWriteLock;
import java.util.function.Supplier;

/**
 * Keeps one exact local executor alive across bounded coordinator publication repair.
 * Coordinator and callback calls are always made without this object's lock.
 */
public final class KorriMoonlightExecutorPublicationRepair implements AutoCloseable {
    private static final int MAX_REGISTRATION_ATTEMPTS = 3;

    public interface Scheduler { void postDelayed(Runnable action, long delayMs); }
    public interface ExactLaunchGuard { boolean isCurrent(); }
    public interface GenerationListener { void changed(String generation); }

    private final Object lock = new Object();
    private final ReentrantReadWriteLock installGate = new ReentrantReadWriteLock(true);
    private final KorriMoonlightActionCoordinator coordinator;
    private final Scheduler scheduler;
    private final ExactLaunchGuard guard;
    private final GenerationListener listener;
    private final String launchId;
    private final KorriMoonlightActionExecutor executor;
    private String generation;
    private long repairToken;
    private boolean repairing;
    private boolean closed;

    public KorriMoonlightExecutorPublicationRepair(
            KorriMoonlightActionCoordinator coordinator,
            Scheduler scheduler,
            ExactLaunchGuard guard,
            GenerationListener listener,
            String launchId,
            KorriMoonlightActionExecutor executor) {
        this.coordinator = coordinator;
        this.scheduler = scheduler;
        this.guard = guard;
        this.listener = listener;
        this.launchId = launchId;
        this.executor = executor;
    }

    public void start() {
        repairMissingRegistration();
    }

    /** Native snapshot truth owns publication for all runtime effects. */
    public void snapshotChanged() {
        if (!guard.isCurrent()) return;
        String current;
        synchronized (lock) {
            if (closed) return;
            current = generation;
        }
        if (current == null) {
            repairMissingRegistration();
            return;
        }
        if (coordinator.republishExact(launchId, current, executor)) return;
        if (!guard.isCurrent()) return;
        synchronized (lock) {
            if (closed || !current.equals(generation)) return;
            generation = null;
            repairing = false;
            repairToken++;
        }
        listener.changed(null);
        repairMissingRegistration();
    }

    public String generation() {
        synchronized (lock) { return generation; }
    }

    private void repairMissingRegistration() {
        long token;
        synchronized (lock) {
            if (closed || generation != null || repairing) return;
            repairing = true;
            token = ++repairToken;
        }
        registerAttempt(token, 0);
    }

    private void registerAttempt(long token, int attempt) {
        if (!guard.isCurrent()) {
            stopRepair(token);
            return;
        }
        synchronized (lock) {
            if (closed || generation != null || !repairing || repairToken != token) return;
        }

        String replacement = coordinator.registerIfAbsent(
                launchId, executor, registrationAuthorization(token));
        if (replacement != null) {
            if (!guard.isCurrent()) {
                coordinator.unregister(launchId, replacement, executor);
                stopRepair(token);
                return;
            }
            boolean discard;
            synchronized (lock) {
                discard = closed || generation != null || !repairing || repairToken != token;
                if (!discard) {
                    generation = replacement;
                    repairing = false;
                }
            }
            if (discard) {
                coordinator.unregister(launchId, replacement, executor);
            } else {
                listener.changed(replacement);
            }
            return;
        }

        if (attempt + 1 >= MAX_REGISTRATION_ATTEMPTS) {
            stopRepair(token);
            return;
        }
        scheduler.postDelayed(
                () -> registerAttempt(token, attempt + 1),
                100L * (attempt + 1));
    }

    private KorriMoonlightActionCoordinator.RegistrationAuthorization
            registrationAuthorization(long token) {
        return new KorriMoonlightActionCoordinator.RegistrationAuthorization() {
            @Override
            public boolean isCurrent() {
                return registrationCurrent(token) && guard.isCurrent();
            }

            @Override
            public <T> T commit(Supplier<T> action, T staleResult) {
                installGate.readLock().lock();
                try {
                    // This is the linearization point against close/replacement. The
                    // lease covers only coordinator publish/install, never stateJson().
                    if (!registrationCurrent(token) || !guard.isCurrent()) {
                        return staleResult;
                    }
                    return action.get();
                } finally {
                    installGate.readLock().unlock();
                }
            }
        };
    }

    private boolean registrationCurrent(long token) {
        synchronized (lock) {
            return !closed && generation == null && repairing && repairToken == token;
        }
    }

    private void stopRepair(long token) {
        synchronized (lock) {
            if (repairToken == token) repairing = false;
        }
    }

    @Override
    public void close() {
        boolean changed = false;
        installGate.writeLock().lock();
        try {
            synchronized (lock) {
                if (closed) return;
                closed = true;
                repairToken++;
                repairing = false;
                generation = null;
                changed = true;
            }
            // A final install that entered first has completed before this write lease.
            // Remove it by exact object identity even if its generation was not copied.
            coordinator.unregisterExactExecutor(launchId, executor);
        } finally {
            installGate.writeLock().unlock();
        }
        if (changed) listener.changed(null);
    }
}
