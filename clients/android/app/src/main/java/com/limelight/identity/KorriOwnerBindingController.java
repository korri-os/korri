package com.limelight.identity;

import android.net.Uri;

import org.json.JSONObject;

/** Coordinates the controller-first owner-binding flow without handling a person private key. */
public final class KorriOwnerBindingController implements PersonSigner.Listener, AutoCloseable {
    public static final String CHANGED_EVENT = "korri-owner-binding-changed";
    private static final String REQUESTED_ACTION = "Bind this device to your person identity";

    public interface NativeIdentity {
        String status();
        String ownerBindingTemplate(long createdAtSeconds);
        String applyOwnerBinding(
                String unsignedTemplateJson,
                String expectedOwnerPublicKey,
                String signedEventJson);
    }

    public interface Listener {
        void onOwnerBindingChanged();
    }

    private final NativeIdentity identity;
    private final Listener listener;
    private PersonSigner signer;
    private PersonSigner.State signerState;

    public KorriOwnerBindingController(NativeIdentity identity, Listener listener) {
        this.identity = identity;
        this.listener = listener;
    }

    public void attachSigner(PersonSigner signer) {
        this.signer = signer;
        this.signerState = signer.state();
    }

    public String snapshotJson() {
        try {
            JSONObject identityState = new JSONObject(identity.status());
            JSONObject result = new JSONObject();
            result.put("identity", identityState);
            result.put("personSigner", signerStateJson());
            String devicePublicKey = identityState.optString("devicePublicKey", "");
            if (!devicePublicKey.isEmpty()) {
                result.put("deviceFingerprint", devicePublicKey);
            }
            result.put("requestedAction", REQUESTED_ACTION);
            if ("Unowned".equals(identityState.optString("_tag"))) {
                String template = identity.ownerBindingTemplate(System.currentTimeMillis() / 1000L);
                result.put("bindingUri", bindingUri(template));
                if (signer == null || !signer.isAvailable()) {
                    result.put("signerRequirement", Nip55PersonSigner.signerRequirement());
                }
            }
            return result.toString();
        } catch (Exception error) {
            return defectSnapshot(error);
        }
    }

    public String startOwnerBinding() {
        if (signer == null) {
            signerState = PersonSigner.State.unavailable(Nip55PersonSigner.signerRequirement());
            listener.onOwnerBindingChanged();
            return snapshotJson();
        }
        try {
            JSONObject current = new JSONObject(identity.status());
            if (!"Unowned".equals(current.optString("_tag"))) return snapshotJson();
            String template = identity.ownerBindingTemplate(System.currentTimeMillis() / 1000L);
            signerState = signer.request(new PersonSigner.Request(template));
        } catch (Exception error) {
            signerState = PersonSigner.State.defect("Korri could not create the owner request");
            listener.onOwnerBindingChanged();
        }
        return snapshotJson();
    }

    public void onActivityResult(int resultCode, android.content.Intent data) {
        if (signer != null) signer.onActivityResult(resultCode, data);
    }

    @Override
    public void onPersonSignerState(PersonSigner.State next) {
        if (next.kind == PersonSigner.Kind.Approved) {
            try {
                String applied = identity.applyOwnerBinding(
                        next.unsignedEventTemplate,
                        next.ownerPublicKey,
                        next.signedEventJson);
                JSONObject state = new JSONObject(applied);
                if (!"Owned".equals(state.optString("_tag"))) {
                    signerState = PersonSigner.State.invalidResponse(
                            "The signer response did not create an owner binding");
                } else {
                    signerState = next;
                }
            } catch (Exception error) {
                signerState = PersonSigner.State.invalidResponse(
                        "The signer returned an event that Korri rejected");
            }
        } else {
            signerState = next;
        }
        listener.onOwnerBindingChanged();
    }

    private JSONObject signerStateJson() throws Exception {
        PersonSigner.State current = signerState != null
                ? signerState
                : PersonSigner.State.unavailable(Nip55PersonSigner.signerRequirement());
        JSONObject result = new JSONObject();
        result.put("_tag", current.kind.name());
        result.put("message", current.message);
        return result;
    }

    static String bindingUri(String unsignedTemplateJson) {
        return "nostrsigner:"
                + Uri.encode(unsignedTemplateJson)
                + "?type=sign_event&returnType=event&compressionType=none";
    }

    private String defectSnapshot(Exception error) {
        try {
            JSONObject result = new JSONObject();
            result.put("identity", new JSONObject().put("_tag", "Invalid"));
            result.put("personSigner", new JSONObject()
                    .put("_tag", "Defect")
                    .put("message", "Korri could not read owner state"));
            result.put("requestedAction", REQUESTED_ACTION);
            return result.toString();
        } catch (Exception impossible) {
            return "{\"identity\":{\"_tag\":\"Invalid\"},"
                    + "\"personSigner\":{\"_tag\":\"Defect\","
                    + "\"message\":\"Korri could not read owner state\"}}";
        }
    }

    @Override
    public void close() {
        if (signer != null) signer.close();
    }
}
