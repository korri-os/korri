package com.limelight.identity;

import android.content.Intent;

/**
 * Korri-owned contract for a person signer. Implementations may use NIP-55 on
 * Android or NIP-46 elsewhere. The person private key is never part of this
 * contract.
 */
public interface PersonSigner extends AutoCloseable {
    enum Kind {
        Unavailable,
        Pending,
        Approved,
        Denied,
        InvalidResponse,
        Defect
    }

    final class State {
        public final Kind kind;
        public final String message;
        public final String ownerPublicKey;
        public final String unsignedEventTemplate;
        public final String signedEventJson;

        private State(
                Kind kind,
                String message,
                String ownerPublicKey,
                String unsignedEventTemplate,
                String signedEventJson) {
            this.kind = kind;
            this.message = message;
            this.ownerPublicKey = ownerPublicKey;
            this.unsignedEventTemplate = unsignedEventTemplate;
            this.signedEventJson = signedEventJson;
        }

        public static State unavailable(String message) {
            return new State(Kind.Unavailable, message, null, null, null);
        }

        public static State pending(String message) {
            return new State(Kind.Pending, message, null, null, null);
        }

        public static State approved(
                String ownerPublicKey,
                String unsignedEventTemplate,
                String signedEventJson) {
            return new State(
                    Kind.Approved,
                    "The signer approved the owner binding",
                    ownerPublicKey,
                    unsignedEventTemplate,
                    signedEventJson);
        }

        public static State denied() {
            return new State(Kind.Denied, "The signer denied the request", null, null, null);
        }

        public static State invalidResponse(String message) {
            return new State(Kind.InvalidResponse, message, null, null, null);
        }

        public static State defect(String message) {
            return new State(Kind.Defect, message, null, null, null);
        }
    }

    final class Request {
        public final String unsignedEventTemplate;

        public Request(String unsignedEventTemplate) {
            this.unsignedEventTemplate = unsignedEventTemplate;
        }
    }

    interface Listener {
        void onPersonSignerState(State state);
    }

    State state();

    boolean isAvailable();

    State request(Request request);

    void onActivityResult(int resultCode, Intent data);

    @Override
    void close();
}
