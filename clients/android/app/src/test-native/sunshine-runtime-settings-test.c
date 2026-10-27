#define _DEFAULT_SOURCE

#include <assert.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sched.h>

#include "SunshineRuntimeSettingsDispatch.h"

static void put16(uint8_t* output, uint16_t value) {
    output[0] = (uint8_t)value;
    output[1] = (uint8_t)(value >> 8);
}

static void put32(uint8_t* output, uint32_t value) {
    output[0] = (uint8_t)value;
    output[1] = (uint8_t)(value >> 8);
    output[2] = (uint8_t)(value >> 16);
    output[3] = (uint8_t)(value >> 24);
}

static uint32_t get32(const uint8_t* input) {
    return (uint32_t)input[0] |
           ((uint32_t)input[1] << 8) |
           ((uint32_t)input[2] << 16) |
           ((uint32_t)input[3] << 24);
}

static void beginActive(SS_RUNTIME_SETTINGS_STATE* state, uint64_t epoch) {
    SsRuntimeSettingsBeginSession(state, epoch);
    SsRuntimeSettingsSetSessionActive(state, true);
}

static void capabilityAck(uint8_t output[64],
                          uint32_t requestId,
                          uint16_t status,
                          uint16_t reason,
                          uint32_t supportedOperations,
                          uint32_t proofGatedOperations,
                          uint32_t currentBitrate,
                          uint32_t currentFps,
                          uint32_t currentWidth,
                          uint32_t currentHeight) {
    memset(output, 0, 64);
    put32(output, requestId);
    put16(output + 4, SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES);
    put16(output + 6, status);
    put16(output + 8, reason);
    put32(output + 12, supportedOperations);
    put32(output + 16, proofGatedOperations);
    if (status == SS_RUNTIME_SETTINGS_STATUS_APPLIED && reason == SS_RUNTIME_SETTINGS_REASON_NONE) {
        put32(output + 20, 500);
        put32(output + 24, 150000);
    }
    // sunshine-korri always publishes the nonzero launch FPS here, including
    // disabled and unsupported-encoder capability outcomes.
    put32(output + 28, 60);
    put32(output + 32, 20000);
    put32(output + 36, 60);
    put32(output + 40, 1920);
    put32(output + 44, 1080);
    put32(output + 48, currentBitrate);
    put32(output + 52, currentFps);
    put32(output + 56, currentWidth);
    put32(output + 60, currentHeight);
}

static void valueAck(uint8_t output[16], uint32_t requestId, uint16_t operation,
                     uint16_t status, uint32_t value, uint16_t reason) {
    memset(output, 0, 16);
    put32(output, requestId);
    put16(output + 4, operation);
    put16(output + 6, status);
    put32(output + 8, value);
    put16(output + 12, reason);
}

static void resolutionAck(uint8_t output[20], uint32_t requestId, uint16_t status,
                          uint32_t width, uint32_t height, uint16_t reason) {
    memset(output, 0, 20);
    put32(output, requestId);
    put16(output + 4, SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION);
    put16(output + 6, status);
    put32(output + 8, width);
    put32(output + 12, height);
    put16(output + 16, reason);
}

static void prepareQuery(SS_RUNTIME_SETTINGS_STATE* state,
                         uint64_t nowMs,
                         uint32_t requestId) {
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t requestLength;
    assert(SsRuntimeSettingsPrepareRequest(state, state->snapshot.sessionEpoch, nowMs, requestId,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &requestLength) == 0);
    assert(requestLength == 8);
}

static void establishCapabilities(SS_RUNTIME_SETTINGS_STATE* state, uint32_t requestId) {
    uint8_t ack[64];
    prepareQuery(state, 0, requestId);
    capabilityAck(ack, requestId, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_NONE, 0x0e, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(state, ack, sizeof(ack)) == 0);
}

static void testEpochGenerationAndEncoding(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t requestLength;
    uint64_t generation;
    const uint8_t expectedQuery[8] = {0x44, 0x33, 0x22, 0x11, 0, 0, 0, 0};

    SsRuntimeSettingsInitialize(&state);
    assert(state.snapshot.version == SS_RUNTIME_SETTINGS_SNAPSHOT_VERSION);
    assert(state.snapshot.generation == 0);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 1, 0, 0, 0,
                                           request, &requestLength) ==
           LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);
    beginActive(&state, 41);
    generation = state.snapshot.generation;
    assert(state.snapshot.sessionEpoch == 41);
    assert(state.snapshot.sessionActive == 1);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 10, 0x11223344,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &requestLength) == 0);
    assert(requestLength == sizeof(expectedQuery));
    assert(memcmp(request, expectedQuery, sizeof(expectedQuery)) == 0);
    assert(state.snapshot.generation > generation);
    SsRuntimeSettingsRecordStreamEnd(&state);
    generation = state.snapshot.generation;
    assert(state.snapshot.sessionActive == 0);
    assert(state.snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED);
    SsRuntimeSettingsRecordStreamEnd(&state);
    assert(state.snapshot.generation == generation);
    SsRuntimeSettingsBeginSession(&state, 42);
    assert(state.snapshot.sessionEpoch == 42);
    assert(state.snapshot.generation > generation);
    assert(state.snapshot.sessionActive == 0);
    assert(state.lastIssuedRequestId == 0);
    SsRuntimeSettingsRecordStreamEnd(&state);
    SsRuntimeSettingsSetSessionActive(&state, true);
    assert(state.snapshot.sessionActive == 0);
}

static void testCapabilityStatusAndMasks(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    SS_RUNTIME_SETTINGS_STATE active;
    uint8_t ack[64];
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t requestLength;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    prepareQuery(&state, 0, 1);
    active = state;

    capabilityAck(ack, 1, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_NONE, 0x0f, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);
    state = active;
    capabilityAck(ack, 1, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED, 0, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);
    state = active;
    capabilityAck(ack, 1, SS_RUNTIME_SETTINGS_STATUS_FAILED,
                  SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER, 0, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);

    state = active;
    capabilityAck(ack, 1, SS_RUNTIME_SETTINGS_STATUS_DISABLED,
                  SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED, 0, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == 0);
    assert(state.snapshot.capabilityReceived == 1);
    assert(state.snapshot.capabilityStatus == SS_RUNTIME_SETTINGS_STATUS_DISABLED);
    assert(state.snapshot.supportedOperations == 0);
    assert(state.snapshot.maxBitrateKbps == 0);
    assert(state.snapshot.maxFps == 60);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           30, 0, request, &requestLength) ==
           LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);

    SsRuntimeSettingsBeginSession(&state, 2);
    SsRuntimeSettingsSetSessionActive(&state, true);
    prepareQuery(&state, 0, 1);
    capabilityAck(ack, 1, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER, 0, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == 0);
    assert(state.snapshot.capabilityStatus == SS_RUNTIME_SETTINGS_STATUS_APPLIED);
    assert(state.snapshot.capabilityReason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER);
    assert(state.snapshot.supportedOperations == 0);
    assert(state.snapshot.minBitrateKbps == 0);
    assert(state.snapshot.maxBitrateKbps == 0);
    assert(state.snapshot.maxFps == 60);
    prepareQuery(&state, 0, 2);
    capabilityAck(ack, 2, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER, 0, 0,
                  20000, 60, 1920, 1080);
    put32(ack + 40, 1280);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 64) == LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);
    assert(state.snapshot.launchWidth == 1920);
}

static void testMutationStatusMatrixAndAuthoritativeCurrent(void) {
    struct CASE {
        uint16_t status;
        uint16_t reason;
        bool valid;
    } cases[] = {
        {SS_RUNTIME_SETTINGS_STATUS_APPLIED, SS_RUNTIME_SETTINGS_REASON_NONE, true},
        {SS_RUNTIME_SETTINGS_STATUS_APPLIED, SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED, false},
        {SS_RUNTIME_SETTINGS_STATUS_DISABLED, SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED, true},
        {SS_RUNTIME_SETTINGS_STATUS_DISABLED, SS_RUNTIME_SETTINGS_REASON_NONE, false},
        {SS_RUNTIME_SETTINGS_STATUS_INVALID, SS_RUNTIME_SETTINGS_REASON_INVALID_PAYLOAD, true},
        {SS_RUNTIME_SETTINGS_STATUS_INVALID, SS_RUNTIME_SETTINGS_REASON_INVALID_BOUNDS, true},
        {SS_RUNTIME_SETTINGS_STATUS_INVALID, SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_OPERATION, true},
        {SS_RUNTIME_SETTINGS_STATUS_INVALID, SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED, false},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_INVALID_BOUNDS, true},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER, true},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_BACKEND, true},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_OPERATION, true},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED, true},
        {SS_RUNTIME_SETTINGS_STATUS_FAILED, SS_RUNTIME_SETTINGS_REASON_NO_ACK, false},
    };

    for (size_t index = 0; index < sizeof(cases) / sizeof(cases[0]); index++) {
        SS_RUNTIME_SETTINGS_STATE state;
        uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
        uint8_t ack[16];
        size_t requestLength;
        SsRuntimeSettingsInitialize(&state);
        beginActive(&state, 1);
        establishCapabilities(&state, 1);
        assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                               SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                                               10000, 0, request, &requestLength) == 0);
        valueAck(ack, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                 cases[index].status,
                 cases[index].status == SS_RUNTIME_SETTINGS_STATUS_APPLIED ? 10000 : 15000,
                 cases[index].reason);
        assert((SsRuntimeSettingsProcessAck(&state, ack, sizeof(ack)) == 0) == cases[index].valid);
        if (cases[index].valid) {
            assert(state.snapshot.currentBitrateKbps ==
                   (cases[index].status == SS_RUNTIME_SETTINGS_STATUS_APPLIED ? 10000u : 15000u));
            assert(state.snapshot.launchBitrateKbps == 20000);
        }
    }
}

static void testRejectedBitrateCurrentBounds(void) {
    const uint32_t invalidValues[] = {0, 499, 150001, UINT32_MAX};

    for (size_t index = 0; index < sizeof(invalidValues) / sizeof(invalidValues[0]); index++) {
        SS_RUNTIME_SETTINGS_STATE state;
        uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
        uint8_t ack[16];
        size_t requestLength;

        SsRuntimeSettingsInitialize(&state);
        beginActive(&state, 1);
        establishCapabilities(&state, 1);
        assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                               SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                                               10000, 0, request, &requestLength) == 0);
        valueAck(ack, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                 SS_RUNTIME_SETTINGS_STATUS_FAILED, invalidValues[index],
                 SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED);
        assert(SsRuntimeSettingsProcessAck(&state, ack, sizeof(ack)) ==
               LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);
        assert(state.snapshot.currentBitrateKbps == 20000);
        assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT);
    }

    {
        SS_RUNTIME_SETTINGS_STATE state;
        uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
        uint8_t queryAck[64];
        uint8_t mutationAck[16];
        size_t requestLength;

        SsRuntimeSettingsInitialize(&state);
        beginActive(&state, 1);
        establishCapabilities(&state, 1);
        assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                               SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                                               10000, 0, request, &requestLength) == 0);
        prepareQuery(&state, 0, 3);
        capabilityAck(queryAck, 3, SS_RUNTIME_SETTINGS_STATUS_DISABLED,
                      SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED, 0, 0,
                      20000, 60, 1920, 1080);
        assert(SsRuntimeSettingsProcessAck(&state, queryAck, sizeof(queryAck)) == 0);
        assert(state.snapshot.minBitrateKbps == 0);
        valueAck(mutationAck, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                 SS_RUNTIME_SETTINGS_STATUS_FAILED, UINT32_MAX,
                 SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED);
        assert(SsRuntimeSettingsProcessAck(&state, mutationAck, sizeof(mutationAck)) ==
               LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK);
    }
}

static void testResolutionRejectionCurrent(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    uint8_t ack[20];
    size_t requestLength;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION,
                                           854, 480, request, &requestLength) == 0);
    resolutionAck(ack, 2, SS_RUNTIME_SETTINGS_STATUS_FAILED,
                  1280, 720, SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED);
    assert(SsRuntimeSettingsProcessAck(&state, ack, sizeof(ack)) == 0);
    assert(state.snapshot.currentWidth == 1280);
    assert(state.snapshot.currentHeight == 720);
    assert(state.snapshot.launchWidth == 1920);
}

static void startMutationAndQuery(SS_RUNTIME_SETTINGS_STATE* state,
                                  uint32_t mutationRequest,
                                  uint32_t queryRequest) {
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t requestLength;
    assert(SsRuntimeSettingsPrepareRequest(state, state->snapshot.sessionEpoch, 0, mutationRequest,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
                                           10000, 0, request, &requestLength) == 0);
    assert(SsRuntimeSettingsPrepareRequest(state, state->snapshot.sessionEpoch, 0, queryRequest,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &requestLength) == 0);
}

static void testAckOrderReconciliation(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t queryAck[64];
    uint8_t mutationAck[16];

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    startMutationAndQuery(&state, 2, 3);
    capabilityAck(queryAck, 3, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_NONE, 0x0e, 0,
                  19999, 59, 1280, 720);
    valueAck(mutationAck, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
             SS_RUNTIME_SETTINGS_STATUS_APPLIED, 10000, SS_RUNTIME_SETTINGS_REASON_NONE);
    assert(SsRuntimeSettingsProcessAck(&state, queryAck, 64) == 0);
    assert(state.snapshot.reconciliationRequired == 1);
    assert(state.snapshot.currentBitrateKbps == 20000);
    assert(SsRuntimeSettingsProcessAck(&state, mutationAck, 16) == 0);
    assert(state.snapshot.currentBitrateKbps == 10000);
    assert(state.snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_APPLIED);
    assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_APPLIED);

    SsRuntimeSettingsBeginSession(&state, 2);
    SsRuntimeSettingsSetSessionActive(&state, true);
    establishCapabilities(&state, 1);
    startMutationAndQuery(&state, 2, 3);
    assert(SsRuntimeSettingsProcessAck(&state, mutationAck, 16) == 0);
    assert(SsRuntimeSettingsProcessAck(&state, queryAck, 64) == 0);
    assert(state.snapshot.currentBitrateKbps == 10000);
    assert(state.snapshot.reconciliationRequired == 1);

    prepareQuery(&state, 0, 4);
    capabilityAck(queryAck, 4, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_NONE, 0x0e, 0,
                  10000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, queryAck, 64) == 0);
    assert(state.snapshot.reconciliationRequired == 0);
    assert(state.snapshot.currentBitrateKbps == 10000);
}

static void testQueryAcceptedBeforeMutation(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    uint8_t queryAck[64];
    uint8_t mutationAck[16];
    size_t requestLength;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    prepareQuery(&state, 0, 2);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 3,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           30, 0, request, &requestLength) == 0);
    capabilityAck(queryAck, 2, SS_RUNTIME_SETTINGS_STATUS_APPLIED,
                  SS_RUNTIME_SETTINGS_REASON_NONE, 0x0e, 0,
                  20000, 60, 1920, 1080);
    assert(SsRuntimeSettingsProcessAck(&state, queryAck, 64) == 0);
    assert(state.snapshot.reconciliationRequired == 1);
    valueAck(mutationAck, 3, SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
             SS_RUNTIME_SETTINGS_STATUS_APPLIED, 30, SS_RUNTIME_SETTINGS_REASON_NONE);
    assert(SsRuntimeSettingsProcessAck(&state, mutationAck, 16) == 0);
    assert(state.snapshot.currentFps == 30);
}

static void testFpsNeverExceedsLaunchLimit(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    uint8_t ack[16];
    size_t requestLength;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    state.snapshot.maxFps = 120;
    assert(state.snapshot.launchFps == 60);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           61, 0, request, &requestLength) ==
           LI_RUNTIME_SETTINGS_ERROR_INVALID_BOUNDS);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 0, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           60, 0, request, &requestLength) == 0);
    valueAck(ack, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
             SS_RUNTIME_SETTINGS_STATUS_APPLIED, 60,
             SS_RUNTIME_SETTINGS_REASON_NONE);
    assert(SsRuntimeSettingsProcessAck(&state, ack, sizeof(ack)) == 0);
    assert(state.snapshot.currentFps == 60);
}

static void testClockRegressionDoesNotTimeout(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t requestLength;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 5000, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           30, 0, request, &requestLength) == 0);
    assert(SsRuntimeSettingsNextTimeoutMs(&state, 4000, 9000) ==
           SS_RUNTIME_SETTINGS_TIMEOUT_MS);
    SsRuntimeSettingsCheckTimeouts(&state, 4000);
    assert(state.mutation.active);
    assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT);
}

static void testTimeoutStaleAndPerLaneTerminal(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    uint8_t ack[16];
    size_t requestLength;
    uint64_t generation;

    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 1);
    establishCapabilities(&state, 1);
    startMutationAndQuery(&state, 2, 3);
    assert(SsRuntimeSettingsNextTimeoutMs(&state, 1000, 9000) == 2000);
    SsRuntimeSettingsCheckTimeouts(&state, 2999);
    assert(state.snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT);
    SsRuntimeSettingsCheckTimeouts(&state, 3000);
    assert(state.snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT);
    assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT);
    assert(state.snapshot.queryRequestId == 3);
    assert(state.snapshot.mutationRequestId == 2);
    generation = state.snapshot.generation;
    valueAck(ack, 2, SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS,
             SS_RUNTIME_SETTINGS_STATUS_APPLIED, 10000, SS_RUNTIME_SETTINGS_REASON_NONE);
    assert(SsRuntimeSettingsProcessAck(&state, ack, 16) == LI_RUNTIME_SETTINGS_ERROR_STALE_ACK);
    assert(state.snapshot.staleAckCount == 1);
    assert(state.snapshot.generation > generation);
    assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT);

    assert(SsRuntimeSettingsPrepareRequest(&state, state.snapshot.sessionEpoch, 4000, 4,
                                           SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
                                           30, 0, request, &requestLength) == 0);
    SsRuntimeSettingsRecordStreamEnd(&state);
    assert(state.snapshot.mutationOutcome == SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED);
    assert(state.snapshot.sessionActive == 0);
    generation = state.snapshot.generation;
    SsRuntimeSettingsRecordStreamEnd(&state);
    assert(state.snapshot.generation == generation);
}

static void testSnapshotWireMapping(void) {
    SS_RUNTIME_SETTINGS_SNAPSHOT snapshot;
    uint64_t wire[SS_RUNTIME_SETTINGS_SNAPSHOT_WIRE_LENGTH];

    memset(&snapshot, 0, sizeof(snapshot));
    snapshot.version = 2;
    snapshot.generation = 101;
    snapshot.sessionEpoch = 102;
    snapshot.sessionActive = 103;
    snapshot.capabilityReceived = 104;
    snapshot.capabilityStatus = 105;
    snapshot.capabilityReason = 106;
    snapshot.supportedOperations = 107;
    snapshot.proofGatedOperations = 108;
    snapshot.minBitrateKbps = 109;
    snapshot.maxBitrateKbps = 110;
    snapshot.maxFps = 111;
    snapshot.launchBitrateKbps = 112;
    snapshot.launchFps = 113;
    snapshot.launchWidth = 114;
    snapshot.launchHeight = 115;
    snapshot.currentBitrateKbps = 116;
    snapshot.currentFps = 117;
    snapshot.currentWidth = 118;
    snapshot.currentHeight = 119;
    snapshot.queryRequestId = 120;
    snapshot.queryOutcome = 121;
    snapshot.queryStatus = 122;
    snapshot.queryReason = 123;
    snapshot.mutationRequestId = 124;
    snapshot.mutationOperation = 125;
    snapshot.mutationOutcome = 126;
    snapshot.mutationStatus = 127;
    snapshot.mutationReason = 128;
    snapshot.staleAckCount = 129;
    snapshot.reconciliationRequired = 130;

    SsRuntimeSettingsSnapshotToWire(&snapshot, wire);
    assert(SS_RUNTIME_SETTINGS_SNAPSHOT_WIRE_LENGTH == 31);
    assert(wire[0] == 2);
    for (size_t index = 1; index < SS_RUNTIME_SETTINGS_SNAPSHOT_WIRE_LENGTH; index++) {
        assert(wire[index] == 100 + index);
    }
}

typedef struct _MOCK_DISPATCH_CONTEXT {
    pthread_mutex_t dispatchMutex;
    pthread_mutex_t sendMutex;
    pthread_cond_t sendCondition;
    uint64_t nowMs;
    uint32_t sentRequestIds[8];
    size_t sentCount;
    atomic_uint lockAttempts;
    atomic_uint readyCalls;
    int readinessError;
    bool blockSend;
    bool sendEntered;
    bool releaseSend;
} MOCK_DISPATCH_CONTEXT;

typedef struct _REQUEST_THREAD {
    SS_RUNTIME_SETTINGS_DISPATCH* dispatch;
    uint32_t requestId;
    uint16_t operation;
    uint32_t value;
    int result;
    atomic_bool started;
} REQUEST_THREAD;

typedef struct _STOP_THREAD {
    SS_RUNTIME_SETTINGS_DISPATCH* dispatch;
    atomic_bool started;
    atomic_bool done;
} STOP_THREAD;

static void mockLock(void* opaque) {
    MOCK_DISPATCH_CONTEXT* context = opaque;
    atomic_fetch_add(&context->lockAttempts, 1);
    assert(pthread_mutex_lock(&context->dispatchMutex) == 0);
}

static void mockUnlock(void* opaque) {
    MOCK_DISPATCH_CONTEXT* context = opaque;
    assert(pthread_mutex_unlock(&context->dispatchMutex) == 0);
}

static int mockReady(void* opaque) {
    MOCK_DISPATCH_CONTEXT* context = opaque;
    atomic_fetch_add(&context->readyCalls, 1);
    return context->readinessError;
}

static uint64_t mockClock(void* opaque) {
    MOCK_DISPATCH_CONTEXT* context = opaque;
    return context->nowMs;
}

static bool mockSend(void* opaque, const uint8_t* payload, size_t payloadLength) {
    MOCK_DISPATCH_CONTEXT* context = opaque;
    assert(payloadLength == 8 || payloadLength == 12 || payloadLength == 16);
    assert(pthread_mutex_lock(&context->sendMutex) == 0);
    context->sentRequestIds[context->sentCount++] = get32(payload);
    context->sendEntered = true;
    assert(pthread_cond_broadcast(&context->sendCondition) == 0);
    while (context->blockSend && !context->releaseSend) {
        assert(pthread_cond_wait(&context->sendCondition, &context->sendMutex) == 0);
    }
    assert(pthread_mutex_unlock(&context->sendMutex) == 0);
    return true;
}

static void* requestThread(void* opaque) {
    REQUEST_THREAD* request = opaque;
    atomic_store(&request->started, true);
    request->result = SsRuntimeSettingsDispatchRequest(request->dispatch, 7, request->requestId,
                                                       request->operation,
                                                       request->value,
                                                       0);
    return NULL;
}

static void* stopThread(void* opaque) {
    STOP_THREAD* stop = opaque;
    atomic_store(&stop->started, true);
    SsRuntimeSettingsDispatchEndSession(stop->dispatch);
    atomic_store(&stop->done, true);
    return NULL;
}

static void waitForSend(MOCK_DISPATCH_CONTEXT* context) {
    assert(pthread_mutex_lock(&context->sendMutex) == 0);
    while (!context->sendEntered) {
        assert(pthread_cond_wait(&context->sendCondition, &context->sendMutex) == 0);
    }
    assert(pthread_mutex_unlock(&context->sendMutex) == 0);
}

static void waitForStarted(atomic_bool* started) {
    while (!atomic_load(started)) {
        sched_yield();
    }
}

static void waitForLockAttempts(MOCK_DISPATCH_CONTEXT* context, unsigned int expected) {
    while (atomic_load(&context->lockAttempts) < expected) {
        sched_yield();
    }
}

static void releaseSend(MOCK_DISPATCH_CONTEXT* context) {
    assert(pthread_mutex_lock(&context->sendMutex) == 0);
    context->releaseSend = true;
    assert(pthread_cond_broadcast(&context->sendCondition) == 0);
    assert(pthread_mutex_unlock(&context->sendMutex) == 0);
}

static void testDispatchLifecycleAndOrdering(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    SS_RUNTIME_SETTINGS_DISPATCH dispatch;
    SS_RUNTIME_SETTINGS_SNAPSHOT snapshot;
    MOCK_DISPATCH_CONTEXT context;
    REQUEST_THREAD first = {
        .dispatch = &dispatch,
        .requestId = 10,
        .operation = SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
        .started = ATOMIC_VAR_INIT(false),
    };
    REQUEST_THREAD second = {
        .dispatch = &dispatch,
        .requestId = 11,
        .operation = SS_RUNTIME_SETTINGS_OPERATION_SET_FPS,
        .value = 30,
        .started = ATOMIC_VAR_INIT(false),
    };
    REQUEST_THREAD third = {
        .dispatch = &dispatch,
        .requestId = 12,
        .operation = SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
        .started = ATOMIC_VAR_INIT(false),
    };
    STOP_THREAD stop = {
        .dispatch = &dispatch,
        .started = ATOMIC_VAR_INIT(false),
        .done = ATOMIC_VAR_INIT(false),
    };
    pthread_t firstThread;
    pthread_t secondThread;
    pthread_t thirdThread;
    pthread_t stopper;
    unsigned int lockAttemptsBeforeConcurrentRequests;
    unsigned int lockAttemptsBeforeStop;

    memset(&context, 0, sizeof(context));
    atomic_init(&context.lockAttempts, 0);
    atomic_init(&context.readyCalls, 0);
    assert(pthread_mutex_init(&context.dispatchMutex, NULL) == 0);
    assert(pthread_mutex_init(&context.sendMutex, NULL) == 0);
    assert(pthread_cond_init(&context.sendCondition, NULL) == 0);
    SsRuntimeSettingsInitialize(&state);
    SsRuntimeSettingsDispatchInitialize(&dispatch, &state, &context,
                                        mockLock, mockUnlock, mockReady, mockSend, mockClock);

    assert(SsRuntimeSettingsDispatchRequest(&dispatch, 7, 1, 0, 0, 0) ==
           LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);
    assert(atomic_load(&context.readyCalls) == 0);
    assert(context.sentCount == 0);
    SsRuntimeSettingsDispatchBeginSession(&dispatch, 7);
    assert(SsRuntimeSettingsDispatchRequest(&dispatch, 7, 1, 0, 0, 0) ==
           LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);
    assert(atomic_load(&context.readyCalls) == 0);

    context.readinessError = LI_RUNTIME_SETTINGS_ERROR_NOT_SUNSHINE;
    SsRuntimeSettingsDispatchSetActive(&dispatch, true);
    SsRuntimeSettingsDispatchGetSnapshot(&dispatch, &snapshot);
    assert(snapshot.sessionActive == 0);
    assert(atomic_load(&context.readyCalls) == 1);
    assert(SsRuntimeSettingsDispatchRequest(&dispatch, 7, 1, 0, 0, 0) ==
           LI_RUNTIME_SETTINGS_ERROR_NOT_SUNSHINE);
    assert(atomic_load(&context.readyCalls) == 1);
    assert(context.sentCount == 0);

    context.readinessError = 0;
    SsRuntimeSettingsDispatchSetActive(&dispatch, true);
    SsRuntimeSettingsDispatchGetSnapshot(&dispatch, &snapshot);
    assert(snapshot.sessionActive == 1);
    assert(atomic_load(&context.readyCalls) == 2);
    assert(SsRuntimeSettingsDispatchRequest(&dispatch, 6, 1, 0, 0, 0) ==
           LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION);
    assert(atomic_load(&context.readyCalls) == 2);
    assert(context.sentCount == 0);
    context.readinessError = LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
    assert(SsRuntimeSettingsDispatchRequest(&dispatch, 7, 1, 0, 0, 0) ==
           LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);
    assert(atomic_load(&context.readyCalls) == 3);
    assert(context.sentCount == 0);
    context.readinessError = 0;
    establishCapabilities(&state, 1);

    context.blockSend = true;
    lockAttemptsBeforeConcurrentRequests = atomic_load(&context.lockAttempts);
    assert(pthread_create(&firstThread, NULL, requestThread, &first) == 0);
    waitForSend(&context);
    assert(pthread_create(&secondThread, NULL, requestThread, &second) == 0);
    waitForStarted(&second.started);
    waitForLockAttempts(&context, lockAttemptsBeforeConcurrentRequests + 2);
    assert(context.sentCount == 1);
    releaseSend(&context);
    assert(pthread_join(firstThread, NULL) == 0);
    assert(pthread_join(secondThread, NULL) == 0);
    assert(first.result == 0);
    assert(second.result == 0);
    assert(context.sentCount == 2);
    assert(context.sentRequestIds[0] == 10);
    assert(context.sentRequestIds[1] == 11);

    SsRuntimeSettingsRecordSendFailure(&state, 10,
                                       SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES);
    SsRuntimeSettingsRecordSendFailure(&state, 11,
                                       SS_RUNTIME_SETTINGS_OPERATION_SET_FPS);
    context.sendEntered = false;
    context.releaseSend = false;
    lockAttemptsBeforeStop = atomic_load(&context.lockAttempts);
    assert(pthread_create(&thirdThread, NULL, requestThread, &third) == 0);
    waitForSend(&context);
    assert(pthread_create(&stopper, NULL, stopThread, &stop) == 0);
    waitForStarted(&stop.started);
    waitForLockAttempts(&context, lockAttemptsBeforeStop + 2);
    assert(!atomic_load(&stop.done));
    releaseSend(&context);
    assert(pthread_join(thirdThread, NULL) == 0);
    assert(pthread_join(stopper, NULL) == 0);
    assert(third.result == 0);
    SsRuntimeSettingsDispatchGetSnapshot(&dispatch, &snapshot);
    assert(snapshot.sessionActive == 0);
    assert(snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED);
    {
        unsigned int readyCallsAfterEnd = atomic_load(&context.readyCalls);
        assert(SsRuntimeSettingsDispatchRequest(&dispatch, 7, 13, 0, 0, 0) ==
               LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY);
        assert(atomic_load(&context.readyCalls) == readyCallsAfterEnd);
    }
    assert(context.sentCount == 3);
    SsRuntimeSettingsDispatchGetSnapshot(&dispatch, &snapshot);
    assert(snapshot.queryRequestId == 12);
    assert(snapshot.queryOutcome == SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED);

    assert(pthread_cond_destroy(&context.sendCondition) == 0);
    assert(pthread_mutex_destroy(&context.sendMutex) == 0);
    assert(pthread_mutex_destroy(&context.dispatchMutex) == 0);
}

static void testExpectedEpochRejectsReplacementRace(void) {
    SS_RUNTIME_SETTINGS_STATE state;
    uint8_t request[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t length;
    SsRuntimeSettingsInitialize(&state);
    beginActive(&state, 9);
    assert(SsRuntimeSettingsPrepareRequest(&state, 8, 0, 1,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &length) ==
           LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION);
    assert(state.lastIssuedRequestId == 0);
    assert(!state.query.active);
    assert(SsRuntimeSettingsPrepareRequest(&state, 9, 0, 1,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &length) == 0);
    SsRuntimeSettingsRecordStreamEnd(&state);
    SsRuntimeSettingsBeginSession(&state, 10);
    SsRuntimeSettingsSetSessionActive(&state, true);
    assert(SsRuntimeSettingsPrepareRequest(&state, 9, 1, 2,
                                           SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES,
                                           0, 0, request, &length) ==
           LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION);
    assert(state.lastIssuedRequestId == 0);
}

int main(void) {
    testExpectedEpochRejectsReplacementRace();
    testEpochGenerationAndEncoding();
    testCapabilityStatusAndMasks();
    testMutationStatusMatrixAndAuthoritativeCurrent();
    testRejectedBitrateCurrentBounds();
    testResolutionRejectionCurrent();
    testAckOrderReconciliation();
    testQueryAcceptedBeforeMutation();
    testFpsNeverExceedsLaunchLimit();
    testClockRegressionDoesNotTimeout();
    testTimeoutStaleAndPerLaneTerminal();
    testSnapshotWireMapping();
    testDispatchLifecycleAndOrdering();
    puts("sunshine runtime-settings native tests passed");
    return 0;
}
