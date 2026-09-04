package com.limelight;

import com.limelight.nvstream.http.ComputerDetails;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.Executor;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.atomic.AtomicBoolean;

/** Seeds Artemis host state from korrid's configured native peer endpoints. */
final class KorriMoonlightHostBootstrap implements AutoCloseable {
    private static final int MAX_CANDIDATES = 16;

    static final class Candidate {
        final String label;
        final ComputerDetails.AddressTuple manualAddress;

        Candidate(String label, ComputerDetails.AddressTuple manualAddress) {
            this.label = requireValue("label", label);
            if (manualAddress == null) {
                throw new IllegalArgumentException("Moonlight host candidate address is invalid");
            }
            this.manualAddress = manualAddress;
        }

        String registrationKey() {
            return label + "\u0000" + manualAddress.address + "\u0000" + manualAddress.port;
        }
    }

    interface Candidates {
        List<Candidate> read() throws Exception;
    }

    interface Registrar {
        boolean add(Candidate candidate) throws Exception;
    }

    interface Completion {
        void hostsChanged();
    }

    private final Candidates candidates;
    private final Registrar registrar;
    private final Completion completion;
    private final Executor executor;
    private final ExecutorService ownedExecutor;
    private final Set<String> registered = Collections.synchronizedSet(new HashSet<>());
    private final AtomicBoolean running = new AtomicBoolean();
    private final AtomicBoolean closed = new AtomicBoolean();

    KorriMoonlightHostBootstrap(
            Candidates candidates,
            Registrar registrar,
            Completion completion) {
        this(candidates, registrar, completion, createExecutor());
    }

    KorriMoonlightHostBootstrap(
            Candidates candidates,
            Registrar registrar,
            Completion completion,
            Executor executor) {
        this.candidates = candidates;
        this.registrar = registrar;
        this.completion = completion;
        this.executor = executor;
        this.ownedExecutor = executor instanceof ExecutorService
                ? (ExecutorService) executor
                : null;
    }

    void start() {
        if (closed.get() || !running.compareAndSet(false, true)) return;
        try {
            executor.execute(this::run);
        } catch (RuntimeException ignored) {
            running.set(false);
            // The next Activity lifecycle may retry. Never block the portal thread.
        }
    }

    private void run() {
        boolean changed = false;
        try {
            for (Candidate candidate : candidates.read()) {
                if (closed.get()) return;
                String registrationKey = candidate.registrationKey();
                if (registered.contains(registrationKey)) continue;
                try {
                    if (registrar.add(candidate)) {
                        registered.add(registrationKey);
                        changed = true;
                    }
                } catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    return;
                } catch (Exception ignored) {
                    // One unreachable configured peer must not suppress the others.
                }
            }
            if (changed && !closed.get()) completion.hostsChanged();
        } catch (Exception ignored) {
            // A later Activity lifecycle retries transient configuration failures.
        } finally {
            running.set(false);
        }
    }

    static List<Candidate> decodeCandidates(String encoded) {
        try {
            JSONObject response = new JSONObject(encoded);
            if (!"Candidates".equals(response.optString("_tag"))) {
                throw new IllegalArgumentException("Moonlight host candidates are unavailable");
            }
            JSONArray items = response.getJSONArray("items");
            if (items.length() > MAX_CANDIDATES) {
                throw new IllegalArgumentException("too many Moonlight host candidates");
            }
            List<Candidate> decoded = new ArrayList<>(items.length());
            for (int index = 0; index < items.length(); index++) {
                try {
                    JSONObject item = items.getJSONObject(index);
                    ComputerDetails.AddressTuple manualAddress =
                            KorriMoonlightAddressParser.parse(item.getString("address"));
                    if (manualAddress != null) {
                        decoded.add(new Candidate(item.getString("label"), manualAddress));
                    }
                } catch (Exception ignored) {
                    // One malformed peer must not suppress other configured hosts.
                }
            }
            if (decoded.isEmpty()) {
                throw new IllegalArgumentException("no valid Moonlight host candidates");
            }
            return Collections.unmodifiableList(decoded);
        } catch (IllegalArgumentException error) {
            throw error;
        } catch (Exception error) {
            throw new IllegalArgumentException("invalid Moonlight host candidates", error);
        }
    }

    private static String requireValue(String name, String value) {
        if (value == null || value.trim().isEmpty()) {
            throw new IllegalArgumentException("Moonlight host candidate " + name + " is empty");
        }
        return value.trim();
    }

    private static ExecutorService createExecutor() {
        ThreadFactory factory = runnable -> {
            Thread thread = new Thread(runnable, "korri-moonlight-host-bootstrap");
            thread.setDaemon(true);
            return thread;
        };
        return Executors.newSingleThreadExecutor(factory);
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
        if (ownedExecutor != null) ownedExecutor.shutdownNow();
    }
}
