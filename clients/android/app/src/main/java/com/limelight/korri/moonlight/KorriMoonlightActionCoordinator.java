package com.limelight.korri.moonlight;


import java.security.SecureRandom;
import java.util.function.Supplier;

/** Process-local exact-launch owner for the current live Artemis executor. */
public final class KorriMoonlightActionCoordinator {
    public interface Publication {
        boolean publish(String stateJson);
        boolean clear(String launchId, String generation);
    }

    /** Final-install lease supplied by the exact repair lifetime. */
    public interface RegistrationAuthorization {
        boolean isCurrent();
        default <T> T commit(Supplier<T> action, T staleResult) {
            if (!isCurrent()) return staleResult;
            return action.get();
        }
    }

    private static final SecureRandom RANDOM = new SecureRandom();

    private static final class Ownership {
        final String launchId;
        final String generation;
        final KorriMoonlightActionExecutor executor;

        Ownership(
                String launchId,
                String generation,
                KorriMoonlightActionExecutor executor) {
            this.launchId = launchId;
            this.generation = generation;
            this.executor = executor;
        }
    }

    private final Publication publication;
    private Ownership owner;

    public KorriMoonlightActionCoordinator(Publication publication) {
        this.publication = publication;
    }

    /** A replacement object gets a fresh process-local identity even for the same launch. */
    public String register(
            String nextLaunchId, KorriMoonlightActionExecutor nextExecutor) {
        if (nextLaunchId == null || nextLaunchId.isEmpty() || nextExecutor == null) return null;

        final Ownership observed;
        synchronized (this) {
            observed = owner;
        }
        String nextGeneration = generation();
        // stateJson() may wait for the UI thread. Never hold the ownership monitor here.
        String state = nextExecutor.stateJson(nextLaunchId, nextGeneration);
        if (state.isEmpty()) {
            clear(invalidateExact(observed));
            return null;
        }

        Ownership invalidated = null;
        boolean installed = false;
        synchronized (this) {
            // A delayed registration attempt cannot replace a newer concurrent owner.
            if (owner != observed) return null;
            if (!publication.publish(state)) {
                invalidated = owner;
                owner = null;
            } else {
                owner = new Ownership(nextLaunchId, nextGeneration, nextExecutor);
                installed = true;
            }
        }
        clear(invalidated);
        return installed ? nextGeneration : null;
    }

    /**
     * Repair registration is compare-and-set. It may recover this exact executor, but it
     * never replaces or invalidates a different current owner.
     */
    public String registerIfAbsent(
            String expectedLaunchId, KorriMoonlightActionExecutor expectedExecutor) {
        return registerIfAbsent(expectedLaunchId, expectedExecutor, () -> true);
    }

    public String registerIfAbsent(
            String expectedLaunchId,
            KorriMoonlightActionExecutor expectedExecutor,
            RegistrationAuthorization authorization) {
        if (expectedLaunchId == null || expectedLaunchId.isEmpty()
                || expectedExecutor == null || authorization == null) {
            return null;
        }
        synchronized (this) {
            if (owner != null) return sameExecutor(owner, expectedLaunchId, expectedExecutor)
                    ? owner.generation : null;
        }
        if (!authorization.isCurrent()) return null;

        String nextGeneration = generation();
        // The absence observed above is only a candidate. State materialization may wait
        // for UI, so it remains outside both the coordinator monitor and repair lease.
        String state = expectedExecutor.stateJson(expectedLaunchId, nextGeneration);
        if (state.isEmpty()) return null;
        return authorization.commit(() -> {
            synchronized (KorriMoonlightActionCoordinator.this) {
                if (owner != null) {
                    return sameExecutor(owner, expectedLaunchId, expectedExecutor)
                            ? owner.generation : null;
                }
                if (!publication.publish(state)) return null;
                owner = new Ownership(expectedLaunchId, nextGeneration, expectedExecutor);
                return nextGeneration;
            }
        }, null);
    }

    /** Late teardown clears only the exact object and opaque generation currently registered. */
    public boolean unregister(
            String expectedLaunchId,
            String expectedGeneration,
            KorriMoonlightActionExecutor expectedExecutor) {
        Ownership removed;
        synchronized (this) {
            if (!matches(owner, expectedLaunchId, expectedGeneration, expectedExecutor)) {
                return false;
            }
            removed = owner;
            owner = null;
        }
        clear(removed);
        return true;
    }

    /** Removes only the current owner for this exact launch and executor object. */
    public boolean unregisterExactExecutor(
            String expectedLaunchId,
            KorriMoonlightActionExecutor expectedExecutor) {
        Ownership removed;
        synchronized (this) {
            if (owner == null || !sameExecutor(owner, expectedLaunchId, expectedExecutor)) {
                return false;
            }
            removed = owner;
            owner = null;
        }
        clear(removed);
        return true;
    }

    /** Refresh live values immediately before exposing controls to an overlay opening. */
    public boolean republish(String expectedLaunchId) {
        final Ownership expected;
        synchronized (this) {
            if (owner == null || !owner.launchId.equals(expectedLaunchId)) return false;
            expected = owner;
        }
        return republishCurrent(expected);
    }

    /** Repair refresh is exact and cannot touch a newer same-launch executor. */
    public boolean republishExact(
            String expectedLaunchId,
            String expectedGeneration,
            KorriMoonlightActionExecutor expectedExecutor) {
        final Ownership expected;
        synchronized (this) {
            if (!matches(owner, expectedLaunchId, expectedGeneration, expectedExecutor)) {
                return false;
            }
            expected = owner;
        }
        return republishCurrent(expected);
    }

    private boolean republishCurrent(Ownership expected) {
        // Materialize outside the ownership monitor so queued UI effects can commit.
        String state = expected.executor.stateJson(expected.launchId, expected.generation);
        Ownership invalidated = null;
        boolean published = false;
        synchronized (this) {
            if (owner != expected) return false;
            if (!state.isEmpty() && publication.publish(state)) {
                published = true;
            } else {
                invalidated = owner;
                owner = null;
            }
        }
        clear(invalidated);
        return published;
    }

    public KorriMoonlightActionExecutor.Outcome execute(
            KorriMoonlightActionExecutor.Request request) {
        return execute(request, () -> true);
    }

    public KorriMoonlightActionExecutor.Outcome execute(
            KorriMoonlightActionExecutor.Request request,
            KorriMoonlightActionExecutor.Authorization callerAuthorization) {
        if (callerAuthorization == null) return KorriMoonlightActionExecutor.Outcome.STALE;
        final Ownership selected;
        synchronized (this) {
            if (owner == null) return KorriMoonlightActionExecutor.Outcome.UNAVAILABLE;
            if (request == null || !"android-moonlight".equals(request.executorId())
                    || !matchesIdentity(owner, request.launchId(), request.generation())) {
                return KorriMoonlightActionExecutor.Outcome.STALE;
            }
            selected = owner;
        }

        KorriMoonlightActionExecutor.Authorization exactAuthorization =
                new KorriMoonlightActionExecutor.Authorization() {
                    @Override
                    public boolean isCurrent() {
                        return callerAuthorization.isCurrent()
                                && KorriMoonlightActionCoordinator.this.isCurrent(selected);
                    }

                    @Override
                    public <T> T commit(java.util.function.Supplier<T> action, T staleResult) {
                        return callerAuthorization.commit(() -> {
                            // This short gate covers only the final effect/native dispatch.
                            // No stateJson()/UI wait occurs while it is held.
                            synchronized (KorriMoonlightActionCoordinator.this) {
                                if (owner != selected) return staleResult;
                                return action.get();
                            }
                        }, staleResult);
                    }
                };
        KorriMoonlightActionExecutor.Outcome outcome = selected.executor.execute(
                request, exactAuthorization);
        if (outcome == KorriMoonlightActionExecutor.Outcome.EXECUTED
                && request.needsStatePublication()) {
            republishCurrent(selected);
        }
        return outcome;
    }

    private Ownership invalidateExact(Ownership expected) {
        if (expected == null) return null;
        synchronized (this) {
            if (owner != expected) return null;
            owner = null;
            return expected;
        }
    }

    private void clear(Ownership invalidated) {
        if (invalidated != null) {
            publication.clear(invalidated.launchId, invalidated.generation);
        }
    }

    private synchronized boolean isCurrent(Ownership expected) {
        return owner == expected;
    }

    private static boolean sameExecutor(
            Ownership value,
            String expectedLaunch,
            KorriMoonlightActionExecutor expectedExecutor) {
        return value.executor == expectedExecutor && value.launchId.equals(expectedLaunch);
    }

    private static boolean matches(
            Ownership value,
            String expectedLaunch,
            String expectedGeneration,
            KorriMoonlightActionExecutor expectedExecutor) {
        return value != null
                && value.executor == expectedExecutor
                && matchesIdentity(value, expectedLaunch, expectedGeneration);
    }

    private static boolean matchesIdentity(
            Ownership value, String expectedLaunch, String expectedGeneration) {
        return value.launchId.equals(expectedLaunch)
                && value.generation.equals(expectedGeneration);
    }

    private static String generation() {
        byte[] bytes = new byte[32];
        RANDOM.nextBytes(bytes);
        StringBuilder encoded = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) encoded.append(String.format("%02x", value & 0xff));
        return encoded.toString();
    }
}
