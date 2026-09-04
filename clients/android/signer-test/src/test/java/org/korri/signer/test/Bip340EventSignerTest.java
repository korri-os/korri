package org.korri.signer.test;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class Bip340EventSignerTest {
    private static final String TEMPLATE = "{\"kind\":30078,\"created_at\":42,"
            + "\"tags\":[[\"d\",\"org.korri.device-owner:device\"],"
            + "[\"device\",\"device\"],[\"status\",\"owned\"]],\"content\":\"\"}";

    @Test
    public void signsTheRequestedEventAndCanProduceAValidWrongEvent() throws Exception {
        assertEquals(
                "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
                Bip340EventSigner.publicKey());
        JSONObject exact = new JSONObject(Bip340EventSigner.signEvent(TEMPLATE, false));
        JSONObject wrong = new JSONObject(Bip340EventSigner.signEvent(TEMPLATE, true));
        assertEquals(30078, exact.getInt("kind"));
        assertEquals(42L, exact.getLong("created_at"));
        assertEquals("", exact.getString("content"));
        assertEquals("wrong", wrong.getString("content"));
        assertEquals(128, exact.getString("sig").length());
        assertNotEquals(exact.getString("id"), wrong.getString("id"));
    }
}
