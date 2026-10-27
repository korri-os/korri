package com.limelight.nvstream.jni;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class SunshineRuntimeSettingsSnapshotTest {
    @Test
    public void decodesEveryVersionTwoField() {
        long[] wire = new long[SunshineRuntimeSettingsSnapshot.WIRE_LENGTH];
        for (int index = 0; index < wire.length; index++) {
            wire[index] = index;
        }
        wire[0] = SunshineRuntimeSettingsSnapshot.VERSION;
        wire[3] = 1;
        wire[4] = 1;
        wire[30] = 1;

        SunshineRuntimeSettingsSnapshot snapshot =
                SunshineRuntimeSettingsSnapshot.fromWire(wire);

        assertEquals(1, snapshot.generation);
        assertEquals(2, snapshot.sessionEpoch);
        assertTrue(snapshot.sessionActive);
        assertTrue(snapshot.capabilityReceived);
        assertEquals(5, snapshot.capabilityStatus);
        assertEquals(6, snapshot.capabilityReason);
        assertEquals(7, snapshot.supportedOperations);
        assertEquals(19, snapshot.currentHeight);
        assertEquals(20, snapshot.queryRequestId);
        assertEquals(24, snapshot.mutationRequestId);
        assertEquals(29, snapshot.staleAckCount);
        assertTrue(snapshot.reconciliationRequired);
    }

    @Test
    public void acceptsSafeInactiveSnapshot() {
        long[] wire = new long[SunshineRuntimeSettingsSnapshot.WIRE_LENGTH];
        wire[0] = SunshineRuntimeSettingsSnapshot.VERSION;

        SunshineRuntimeSettingsSnapshot snapshot =
                SunshineRuntimeSettingsSnapshot.fromWire(wire);

        assertFalse(snapshot.sessionActive);
        assertFalse(snapshot.capabilityReceived);
        assertEquals(0, snapshot.queryOutcome);
        assertEquals(0, snapshot.mutationOutcome);
    }

    @Test
    public void rejectsWrongLengthAndVersion() {
        expectInvalid(new long[SunshineRuntimeSettingsSnapshot.WIRE_LENGTH - 1]);
        long[] wrongVersion = new long[SunshineRuntimeSettingsSnapshot.WIRE_LENGTH];
        wrongVersion[0] = 1;
        expectInvalid(wrongVersion);
    }

    private static void expectInvalid(long[] wire) {
        try {
            SunshineRuntimeSettingsSnapshot.fromWire(wire);
            fail("Expected invalid snapshot");
        }
        catch (IllegalArgumentException expected) {
            // Expected.
        }
    }
}
