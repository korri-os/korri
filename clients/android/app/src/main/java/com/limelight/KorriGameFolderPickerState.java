package com.limelight;

import org.json.JSONObject;

/**
 * Versioned, re-readable Android game-folder picker state.
 *
 * The WebView never receives a filesystem path. Android validates the picker
 * result, asks embedded korrid for a one-use receipt, and publishes only that
 * receipt in a generation-tagged snapshot until the portal acknowledges it.
 */
final class KorriGameFolderPickerState {
    static final int CONTRACT_VERSION = 1;

    private long generation = 0;
    private State state = State.idle();

    synchronized String choose() {
        if (state.busy) {
            return busyResultJson(generation, state);
        }
        generation += 1;
        state = State.choosing();
        return openResultJson(generation);
    }

    synchronized void selected(String receipt) {
        generation += 1;
        state = State.selected(receipt);
    }

    synchronized void cancelled() {
        generation += 1;
        state = State.cancelled();
    }

    synchronized void problem(String code, String message) {
        generation += 1;
        state = State.problem(code, message);
    }

    synchronized String snapshotJson() {
        return snapshotJson(generation, state);
    }

    synchronized String acknowledgeJson(String generationText) {
        long acknowledged;
        try {
            acknowledged = Long.parseLong(generationText);
        } catch (NumberFormatException error) {
            return staleJson(generation);
        }
        if (acknowledged != generation || !state.definitive) {
            return staleJson(generation);
        }
        generation += 1;
        state = State.idle();
        return acknowledgedJson(generation);
    }

    private static String openResultJson(long generation) {
        try {
            JSONObject result = new JSONObject();
            result.put("_tag", "Opened");
            result.put("generation", Long.toString(generation));
            return result.toString();
        } catch (Exception error) {
            return "{\"_tag\":\"Unavailable\",\"message\":\"open failed\"}";
        }
    }

    private static String busyResultJson(long generation, State state) {
        try {
            JSONObject result = new JSONObject();
            result.put("_tag", "Busy");
            result.put("generation", Long.toString(generation));
            result.put("state", state.tag);
            return result.toString();
        } catch (Exception error) {
            return "{\"_tag\":\"Unavailable\",\"message\":\"open failed\"}";
        }
    }

    static String unavailableJson(String message) {
        try {
            JSONObject result = new JSONObject();
            result.put("_tag", "Unavailable");
            result.put("message", message);
            return result.toString();
        } catch (Exception error) {
            return "{\"_tag\":\"Unavailable\",\"message\":\"unavailable\"}";
        }
    }

    private static String acknowledgedJson(long generation) {
        try {
            JSONObject result = new JSONObject();
            result.put("_tag", "Acknowledged");
            result.put("generation", Long.toString(generation));
            return result.toString();
        } catch (Exception error) {
            return "{\"_tag\":\"Acknowledged\",\"generation\":\"0\"}";
        }
    }

    private static String staleJson(long generation) {
        try {
            JSONObject result = new JSONObject();
            result.put("_tag", "Stale");
            result.put("generation", Long.toString(generation));
            return result.toString();
        } catch (Exception error) {
            return "{\"_tag\":\"Stale\",\"generation\":\"0\"}";
        }
    }

    private static String snapshotJson(long generation, State state) {
        try {
            JSONObject snapshot = new JSONObject();
            snapshot.put("version", CONTRACT_VERSION);
            snapshot.put("generation", Long.toString(generation));
            JSONObject payload = new JSONObject();
            payload.put("_tag", state.tag);
            if (state.receipt != null) payload.put("receipt", state.receipt);
            if (state.code != null) payload.put("code", state.code);
            if (state.message != null) payload.put("message", state.message);
            snapshot.put("state", payload);
            return snapshot.toString();
        } catch (Exception error) {
            return "{\"version\":1,\"generation\":\"0\",\"state\":{\"_tag\":\"Problem\",\"code\":\"SerializationFailed\",\"message\":\"snapshot failed\"}}";
        }
    }

    private static final class State {
        final String tag;
        final String receipt;
        final String code;
        final String message;
        final boolean definitive;
        final boolean busy;

        private State(
                String tag,
                String receipt,
                String code,
                String message,
                boolean definitive,
                boolean busy) {
            this.tag = tag;
            this.receipt = receipt;
            this.code = code;
            this.message = message;
            this.definitive = definitive;
            this.busy = busy;
        }

        static State idle() {
            return new State("Idle", null, null, null, false, false);
        }

        static State choosing() {
            return new State("Choosing", null, null, null, false, true);
        }

        static State selected(String receipt) {
            return new State("Selected", receipt, null, null, true, true);
        }

        static State cancelled() {
            return new State("Cancelled", null, null, null, true, false);
        }

        static State problem(String code, String message) {
            return new State("Problem", null, code, message, true, false);
        }
    }
}
