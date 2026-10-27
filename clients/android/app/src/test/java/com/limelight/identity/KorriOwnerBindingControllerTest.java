package com.limelight.identity;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class KorriOwnerBindingControllerTest {
    private static final String DEVICE = "22".repeat(32);
    private static final String OWNER = "11".repeat(32);
    private static final String TEMPLATE = "{\"kind\":30078,\"created_at\":1,\"tags\":[],\"content\":\"\"}";

    @Test
    public void unownedSnapshotShowsFingerprintActionUriAndSignerRequirement() throws Exception {
        ConfigurableIdentity identity = new ConfigurableIdentity();
        RecordingOwnerListener listener = new RecordingOwnerListener();
        KorriOwnerBindingController controller = new KorriOwnerBindingController(identity, listener);
        controller.attachSigner(new ConfigurablePersonSigner(false));

        JSONObject snapshot = new JSONObject(controller.snapshotJson());
        assertEquals("Unowned", snapshot.getJSONObject("identity").getString("_tag"));
        assertEquals(DEVICE, snapshot.getString("deviceFingerprint"));
        assertEquals("Bind this device to your person identity",
                snapshot.getString("requestedAction"));
        assertTrue(snapshot.getString("bindingUri").startsWith("nostrsigner:"));
        assertTrue(snapshot.getString("signerRequirement").contains("Amber"));
    }

    @Test
    public void approvedPublicEventBecomesOwnedOnlyAfterNativeVerification() throws Exception {
        ConfigurableIdentity identity = new ConfigurableIdentity();
        KorriOwnerBindingController controller = new KorriOwnerBindingController(
                identity, new RecordingOwnerListener());
        ConfigurablePersonSigner signer = new ConfigurablePersonSigner(true);
        controller.attachSigner(signer);

        controller.startOwnerBinding();
        controller.onPersonSignerState(
                PersonSigner.State.approved(OWNER, TEMPLATE, "{\"signed\":true}"));

        JSONObject snapshot = new JSONObject(controller.snapshotJson());
        assertEquals("Owned", snapshot.getJSONObject("identity").getString("_tag"));
        assertEquals("Approved", snapshot.getJSONObject("personSigner").getString("_tag"));
        assertEquals(TEMPLATE, identity.appliedTemplate);
        assertEquals(OWNER, identity.appliedOwner);
        assertEquals("{\"signed\":true}", identity.appliedEvent);
    }

    @Test
    public void wrongSignedEventIsAnInvalidResponseAndDoesNotBind() throws Exception {
        ConfigurableIdentity identity = new ConfigurableIdentity();
        identity.rejectApply = true;
        KorriOwnerBindingController controller = new KorriOwnerBindingController(
                identity, new RecordingOwnerListener());
        ConfigurablePersonSigner signer = new ConfigurablePersonSigner(true);
        controller.attachSigner(signer);

        controller.onPersonSignerState(
                PersonSigner.State.approved(OWNER, TEMPLATE, "{\"wrong\":true}"));

        JSONObject snapshot = new JSONObject(controller.snapshotJson());
        assertEquals("Unowned", snapshot.getJSONObject("identity").getString("_tag"));
        assertEquals("InvalidResponse",
                snapshot.getJSONObject("personSigner").getString("_tag"));
    }

    static final class ConfigurableIdentity implements KorriOwnerBindingController.NativeIdentity {
        boolean owned;
        boolean rejectApply;
        String appliedTemplate;
        String appliedOwner;
        String appliedEvent;

        @Override
        public String status() {
            return owned
                    ? "{\"_tag\":\"Owned\",\"devicePublicKey\":\"" + DEVICE
                        + "\",\"ownerPublicKey\":\"" + OWNER + "\","
                        + "\"eventId\":\"event\",\"createdAt\":1}"
                    : "{\"_tag\":\"Unowned\",\"devicePublicKey\":\"" + DEVICE + "\"}";
        }

        @Override
        public String ownerBindingTemplate(long createdAtSeconds) {
            return TEMPLATE;
        }

        @Override
        public String applyOwnerBinding(
                String unsignedTemplateJson,
                String expectedOwnerPublicKey,
                String signedEventJson) {
            if (rejectApply) throw new IllegalArgumentException("wrong event");
            appliedTemplate = unsignedTemplateJson;
            appliedOwner = expectedOwnerPublicKey;
            appliedEvent = signedEventJson;
            owned = true;
            return status();
        }
    }

    static final class ConfigurablePersonSigner implements PersonSigner {
        private final boolean available;
        private State state = State.unavailable("not requested");

        ConfigurablePersonSigner(boolean available) {
            this.available = available;
        }

        @Override public State state() { return state; }
        @Override public boolean isAvailable() { return available; }
        @Override public State request(Request request) {
            state = available ? State.pending("waiting") : State.unavailable("not installed");
            return state;
        }
        @Override public void onActivityResult(int resultCode, android.content.Intent data) {}
        @Override public void close() {}

    }

    static final class RecordingOwnerListener implements KorriOwnerBindingController.Listener {
        int changes;
        @Override public void onOwnerBindingChanged() { changes++; }
    }
}
