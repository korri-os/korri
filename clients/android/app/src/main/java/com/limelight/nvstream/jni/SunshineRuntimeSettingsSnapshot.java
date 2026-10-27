package com.limelight.nvstream.jni;

import java.util.Arrays;

public final class SunshineRuntimeSettingsSnapshot {
    public static final int VERSION = 2;
    public static final int WIRE_LENGTH = 31;

    public final long generation;
    public final long sessionEpoch;
    public final boolean sessionActive;
    public final boolean capabilityReceived;
    public final int capabilityStatus;
    public final int capabilityReason;
    public final long supportedOperations;
    public final long proofGatedOperations;
    public final long minBitrateKbps;
    public final long maxBitrateKbps;
    public final long maxFps;
    public final long launchBitrateKbps;
    public final long launchFps;
    public final long launchWidth;
    public final long launchHeight;
    public final long currentBitrateKbps;
    public final long currentFps;
    public final long currentWidth;
    public final long currentHeight;
    public final long queryRequestId;
    public final int queryOutcome;
    public final int queryStatus;
    public final int queryReason;
    public final long mutationRequestId;
    public final int mutationOperation;
    public final int mutationOutcome;
    public final int mutationStatus;
    public final int mutationReason;
    public final long staleAckCount;
    public final boolean reconciliationRequired;

    private SunshineRuntimeSettingsSnapshot(long[] values) {
        generation = values[1];
        sessionEpoch = values[2];
        sessionActive = values[3] != 0;
        capabilityReceived = values[4] != 0;
        capabilityStatus = (int) values[5];
        capabilityReason = (int) values[6];
        supportedOperations = values[7];
        proofGatedOperations = values[8];
        minBitrateKbps = values[9];
        maxBitrateKbps = values[10];
        maxFps = values[11];
        launchBitrateKbps = values[12];
        launchFps = values[13];
        launchWidth = values[14];
        launchHeight = values[15];
        currentBitrateKbps = values[16];
        currentFps = values[17];
        currentWidth = values[18];
        currentHeight = values[19];
        queryRequestId = values[20];
        queryOutcome = (int) values[21];
        queryStatus = (int) values[22];
        queryReason = (int) values[23];
        mutationRequestId = values[24];
        mutationOperation = (int) values[25];
        mutationOutcome = (int) values[26];
        mutationStatus = (int) values[27];
        mutationReason = (int) values[28];
        staleAckCount = values[29];
        reconciliationRequired = values[30] != 0;
    }

    public static SunshineRuntimeSettingsSnapshot fromWire(long[] values) {
        if (values == null || values.length != WIRE_LENGTH) {
            throw new IllegalArgumentException("Runtime-settings snapshot has invalid length");
        }
        if (values[0] != VERSION) {
            throw new IllegalArgumentException("Runtime-settings snapshot has unsupported version");
        }
        return new SunshineRuntimeSettingsSnapshot(Arrays.copyOf(values, values.length));
    }
}
