package com.limelight;

import com.limelight.nvstream.http.NvHTTP;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executor;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

/** Seeds Artemis host state from korrid's configured native peer endpoints. */
final class KorriMoonlightHostBootstrap implements AutoCloseable {
    private static final int MAX_CANDIDATES = 16;

    static final class Candidate {
        final String label;
        final String address;

        Candidate(String label, String address) {
            this.label = requireValue("label", label);
            this.address = requireValue("address", address);
        }

        String registrationKey() {
            return label + "\u0000" + address;
        }
    }

    interface Candidates {
        List<Candidate> read() throws Exception;
    }

    interface Commit<T> {
        T run() throws Exception;
    }

    interface Guard {
        boolean current();

        <T> T commit(Commit<T> action) throws Exception;
    }

    interface Registrar {
        boolean add(Candidate candidate, Guard guard) throws Exception;
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
    private final Map<String, String> labelsByAddress = new ConcurrentHashMap<>();
    private final AtomicBoolean running = new AtomicBoolean();
    private final AtomicLong generation = new AtomicLong(1);
    private final Object lifecycleMonitor = new Object();
    private boolean closed;

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
        final long ticket;
        synchronized (lifecycleMonitor) {
            if (closed || !running.compareAndSet(false, true)) return;
            ticket = generation.get();
        }
        try {
            executor.execute(() -> run(ticket));
        } catch (RuntimeException ignored) {
            running.set(false);
            // The next Activity lifecycle may retry. Never block the portal thread.
        }
    }

    private void run(long ticket) {
        Guard guard = guard(ticket);
        boolean changed = false;
        try {
            for (Candidate candidate : candidates.read()) {
                if (!guard.current()) return;
                String registrationKey = candidate.registrationKey();
                if (registered.contains(registrationKey)) continue;
                try {
                    if (registrar.add(candidate, guard) && guard.current()) {
                        registered.add(registrationKey);
                        labelsByAddress.put(
                                addressKey(candidate.address, NvHTTP.DEFAULT_HTTP_PORT),
                                candidate.label);
                        changed = true;
                    }
                } catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    return;
                } catch (Exception ignored) {
                    // One unreachable configured peer must not suppress the others.
                }
            }
            if (changed && guard.current()) completion.hostsChanged();
        } catch (Exception ignored) {
            // A later Activity lifecycle retries transient configuration failures.
        } finally {
            running.set(false);
        }
    }

    private Guard guard(long ticket) {
        return new Guard() {
            @Override
            public boolean current() {
                synchronized (lifecycleMonitor) {
                    return !closed && generation.get() == ticket;
                }
            }

            @Override
            public <T> T commit(Commit<T> action) throws Exception {
                synchronized (lifecycleMonitor) {
                    if (closed || generation.get() != ticket) return null;
                    return action.run();
                }
            }
        };
    }

    String labelForAddress(String address, int port) {
        if (address == null || port <= 0) return null;
        return labelsByAddress.get(addressKey(address, port));
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
                    decoded.add(new Candidate(
                            item.getString("label"),
                            item.getString("address")));
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

    private static String addressKey(String address, int port) {
        String normalized = address.trim().toLowerCase(Locale.ROOT);
        if (normalized.startsWith("[") && normalized.endsWith("]")) {
            normalized = normalized.substring(1, normalized.length() - 1);
        }
        return normalized + "\u0000" + port;
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
        synchronized (lifecycleMonitor) {
            if (closed) return;
            closed = true;
            generation.incrementAndGet();
        }
        if (ownedExecutor != null) ownedExecutor.shutdownNow();
    }
}
