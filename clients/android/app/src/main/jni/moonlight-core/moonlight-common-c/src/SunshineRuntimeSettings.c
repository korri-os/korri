#include "SunshineRuntimeSettings.h"

#include <limits.h>
#include <string.h>

#define SS_RUNTIME_SETTINGS_OPERATION_MAX SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION
#define SS_RUNTIME_SETTINGS_MUTATION_OPERATION_MASK 0x0Eu

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

static uint16_t get16(const uint8_t* input) {
    return (uint16_t)input[0] | ((uint16_t)input[1] << 8);
}

static uint32_t get32(const uint8_t* input) {
    return (uint32_t)input[0] |
           ((uint32_t)input[1] << 8) |
           ((uint32_t)input[2] << 16) |
           ((uint32_t)input[3] << 24);
}

static void changed(SS_RUNTIME_SETTINGS_STATE* state) {
    state->snapshot.generation++;
}

static bool resolutionWithinLaunch(uint32_t launchWidth,
                                   uint32_t launchHeight,
                                   uint32_t width,
                                   uint32_t height) {
    uint64_t left;
    uint64_t right;
    uint64_t delta;
    uint64_t tolerance;

    if (width == 0 || height == 0 || (width % 2) != 0 || (height % 2) != 0 ||
            launchWidth == 0 || launchHeight == 0 ||
            width > launchWidth || height > launchHeight) {
        return false;
    }

    left = (uint64_t)width * launchHeight;
    right = (uint64_t)height * launchWidth;
    delta = left > right ? left - right : right - left;
    tolerance = 2ULL * (launchWidth + launchHeight);
    return delta <= tolerance;
}

static bool requestedValueWithinBounds(const SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot,
                                       uint16_t operation,
                                       uint32_t value,
                                       uint32_t secondaryValue) {
    switch (operation) {
    case SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS:
        return value != 0 && value >= snapshot->minBitrateKbps &&
               snapshot->maxBitrateKbps >= snapshot->minBitrateKbps &&
               value <= snapshot->maxBitrateKbps;
    case SS_RUNTIME_SETTINGS_OPERATION_SET_FPS:
        return value != 0 && snapshot->maxFps != 0 && snapshot->launchFps != 0 &&
               value <= snapshot->maxFps && value <= snapshot->launchFps;
    case SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION:
        return resolutionWithinLaunch(snapshot->launchWidth, snapshot->launchHeight,
                                      value, secondaryValue);
    default:
        return false;
    }
}

static bool currentValueSane(const SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot,
                             const SS_RUNTIME_SETTINGS_COMMAND* command,
                             uint16_t operation,
                             uint32_t value,
                             uint32_t secondaryValue) {
    switch (operation) {
    case SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS:
        if (value == 0) {
            return false;
        }
        if (command->minBitrateKbpsAtAcceptance != 0 ||
                command->maxBitrateKbpsAtAcceptance != 0) {
            return command->minBitrateKbpsAtAcceptance != 0 &&
                   command->maxBitrateKbpsAtAcceptance >= command->minBitrateKbpsAtAcceptance &&
                   value >= command->minBitrateKbpsAtAcceptance &&
                   value <= command->maxBitrateKbpsAtAcceptance;
        }
        return true;
    case SS_RUNTIME_SETTINGS_OPERATION_SET_FPS:
        return value != 0 && snapshot->launchFps != 0 && value <= snapshot->launchFps;
    case SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION:
        return resolutionWithinLaunch(snapshot->launchWidth, snapshot->launchHeight,
                                      value, secondaryValue);
    default:
        return false;
    }
}

static SS_RUNTIME_SETTINGS_COMMAND* commandForOperation(SS_RUNTIME_SETTINGS_STATE* state,
                                                         uint16_t operation) {
    return operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES ?
        &state->query : &state->mutation;
}

static void publishQuery(SS_RUNTIME_SETTINGS_STATE* state,
                         uint32_t requestId,
                         uint32_t outcome,
                         uint16_t status,
                         uint16_t reason) {
    state->snapshot.queryRequestId = requestId;
    state->snapshot.queryOutcome = outcome;
    state->snapshot.queryStatus = status;
    state->snapshot.queryReason = reason;
    changed(state);
}

static void publishMutation(SS_RUNTIME_SETTINGS_STATE* state,
                            uint32_t requestId,
                            uint16_t operation,
                            uint32_t outcome,
                            uint16_t status,
                            uint16_t reason) {
    state->snapshot.mutationRequestId = requestId;
    state->snapshot.mutationOperation = operation;
    state->snapshot.mutationOutcome = outcome;
    state->snapshot.mutationStatus = status;
    state->snapshot.mutationReason = reason;
    changed(state);
}

static void publishCommand(SS_RUNTIME_SETTINGS_STATE* state,
                           uint16_t operation,
                           uint32_t requestId,
                           uint32_t outcome,
                           uint16_t status,
                           uint16_t reason) {
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        publishQuery(state, requestId, outcome, status, reason);
    }
    else {
        publishMutation(state, requestId, operation, outcome, status, reason);
    }
}

void SsRuntimeSettingsInitialize(SS_RUNTIME_SETTINGS_STATE* state) {
    memset(state, 0, sizeof(*state));
    state->snapshot.version = SS_RUNTIME_SETTINGS_SNAPSHOT_VERSION;
    state->snapshot.capabilityStatus = SS_RUNTIME_SETTINGS_STATUS_APPLIED;
    state->snapshot.capabilityReason = SS_RUNTIME_SETTINGS_REASON_NONE;
    state->snapshot.queryStatus = SS_RUNTIME_SETTINGS_STATUS_APPLIED;
    state->snapshot.queryReason = SS_RUNTIME_SETTINGS_REASON_NONE;
    state->snapshot.mutationStatus = SS_RUNTIME_SETTINGS_STATUS_APPLIED;
    state->snapshot.mutationReason = SS_RUNTIME_SETTINGS_REASON_NONE;
}

void SsRuntimeSettingsBeginSession(SS_RUNTIME_SETTINGS_STATE* state, uint64_t sessionEpoch) {
    uint64_t generation = state->snapshot.generation;
    SsRuntimeSettingsInitialize(state);
    state->snapshot.generation = generation + 1;
    state->snapshot.sessionEpoch = sessionEpoch;
}

void SsRuntimeSettingsSetSessionActive(SS_RUNTIME_SETTINGS_STATE* state, bool active) {
    uint32_t activeValue = active && !state->sessionEnded ? 1u : 0u;
    if (state->snapshot.sessionActive != activeValue) {
        state->snapshot.sessionActive = activeValue;
        changed(state);
    }
}

int SsRuntimeSettingsPrepareRequest(SS_RUNTIME_SETTINGS_STATE* state,
                                    uint64_t expectedSessionEpoch,
                                    uint64_t nowMs,
                                    uint32_t requestId,
                                    uint16_t operation,
                                    uint32_t value,
                                    uint32_t secondaryValue,
                                    uint8_t output[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES],
                                    size_t* outputLength) {
    SS_RUNTIME_SETTINGS_COMMAND* command;

    if (!state->snapshot.sessionActive) {
        return LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
    }
    if (expectedSessionEpoch == 0 || expectedSessionEpoch != state->snapshot.sessionEpoch) {
        return LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION;
    }
    if (operation > SS_RUNTIME_SETTINGS_OPERATION_MAX) {
        return LI_RUNTIME_SETTINGS_ERROR_UNSUPPORTED_OPERATION;
    }
    if (requestId == 0 || requestId <= state->lastIssuedRequestId) {
        return LI_RUNTIME_SETTINGS_ERROR_CONFLICT;
    }

    command = commandForOperation(state, operation);
    if (command->active) {
        return LI_RUNTIME_SETTINGS_ERROR_CONFLICT;
    }

    if (operation != SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        uint32_t operationBit = 1u << operation;
        if (!state->snapshot.capabilityReceived ||
                state->snapshot.capabilityStatus != SS_RUNTIME_SETTINGS_STATUS_APPLIED) {
            return LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
        }
        if ((state->snapshot.proofGatedOperations & operationBit) != 0 ||
                (state->snapshot.supportedOperations & operationBit) == 0) {
            return LI_RUNTIME_SETTINGS_ERROR_UNSUPPORTED_OPERATION;
        }
        if (!requestedValueWithinBounds(&state->snapshot, operation, value, secondaryValue)) {
            return LI_RUNTIME_SETTINGS_ERROR_INVALID_BOUNDS;
        }
    }

    memset(output, 0, SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES);
    put32(output, requestId);
    put16(output + 4, operation);
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        *outputLength = 8;
    }
    else if (operation == SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION) {
        put32(output + 8, value);
        put32(output + 12, secondaryValue);
        *outputLength = 16;
    }
    else {
        put32(output + 8, value);
        *outputLength = 12;
    }

    command->active = true;
    command->requestId = requestId;
    command->operation = operation;
    command->requestedValue = value;
    command->requestedSecondaryValue = secondaryValue;
    command->sentAtMs = nowMs;
    command->mutationEpochAtAcceptance = state->mutationEpoch;
    command->mutationWasActiveAtAcceptance = state->mutation.active;
    command->minBitrateKbpsAtAcceptance = state->snapshot.minBitrateKbps;
    command->maxBitrateKbpsAtAcceptance = state->snapshot.maxBitrateKbps;
    if (operation != SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        state->mutationEpoch++;
        command->mutationEpochAtAcceptance = state->mutationEpoch;
    }
    state->lastIssuedRequestId = requestId;
    publishCommand(state, operation, requestId, SS_RUNTIME_SETTINGS_OUTCOME_IN_FLIGHT,
                   SS_RUNTIME_SETTINGS_STATUS_APPLIED, SS_RUNTIME_SETTINGS_REASON_NONE);
    return 0;
}

void SsRuntimeSettingsRecordSendFailure(SS_RUNTIME_SETTINGS_STATE* state,
                                        uint32_t requestId,
                                        uint16_t operation) {
    SS_RUNTIME_SETTINGS_COMMAND* command;

    if (operation > SS_RUNTIME_SETTINGS_OPERATION_MAX) {
        return;
    }
    command = commandForOperation(state, operation);
    if (command->active && command->requestId == requestId && command->operation == operation) {
        command->active = false;
        publishCommand(state, operation, requestId, SS_RUNTIME_SETTINGS_OUTCOME_SEND_FAILED,
                       SS_RUNTIME_SETTINGS_STATUS_FAILED,
                       SS_RUNTIME_SETTINGS_REASON_CONTROL_NOT_READY);
    }
}

static bool hostStatusReasonValid(uint16_t operation, uint16_t status, uint16_t reason) {
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        return (status == SS_RUNTIME_SETTINGS_STATUS_APPLIED &&
                    (reason == SS_RUNTIME_SETTINGS_REASON_NONE ||
                     reason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER)) ||
               (status == SS_RUNTIME_SETTINGS_STATUS_DISABLED &&
                    reason == SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED);
    }

    if (status == SS_RUNTIME_SETTINGS_STATUS_APPLIED) {
        return reason == SS_RUNTIME_SETTINGS_REASON_NONE;
    }
    if (status == SS_RUNTIME_SETTINGS_STATUS_DISABLED) {
        return reason == SS_RUNTIME_SETTINGS_REASON_GATE_DISABLED;
    }
    if (status == SS_RUNTIME_SETTINGS_STATUS_INVALID) {
        return reason == SS_RUNTIME_SETTINGS_REASON_INVALID_PAYLOAD ||
               reason == SS_RUNTIME_SETTINGS_REASON_INVALID_BOUNDS ||
               reason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_OPERATION;
    }
    if (status == SS_RUNTIME_SETTINGS_STATUS_FAILED) {
        return reason == SS_RUNTIME_SETTINGS_REASON_INVALID_BOUNDS ||
               reason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER ||
               reason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_BACKEND ||
               reason == SS_RUNTIME_SETTINGS_REASON_UNSUPPORTED_OPERATION ||
               reason == SS_RUNTIME_SETTINGS_REASON_APPLY_FAILED;
    }
    return false;
}

static bool decodeAckHeader(const uint8_t* payload,
                            size_t payloadLength,
                            uint32_t* requestId,
                            uint16_t* operation,
                            uint16_t* status,
                            uint16_t* reason) {
    size_t expectedLength;
    uint16_t reserved;

    if (payloadLength < 8) {
        return false;
    }

    *requestId = get32(payload);
    *operation = get16(payload + 4);
    *status = get16(payload + 6);
    if (*operation > SS_RUNTIME_SETTINGS_OPERATION_MAX) {
        return false;
    }

    if (*operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        expectedLength = 64;
        if (payloadLength != expectedLength) {
            return false;
        }
        *reason = get16(payload + 8);
        reserved = get16(payload + 10);
    }
    else if (*operation == SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION) {
        expectedLength = 20;
        if (payloadLength != expectedLength) {
            return false;
        }
        *reason = get16(payload + 16);
        reserved = get16(payload + 18);
    }
    else {
        expectedLength = 16;
        if (payloadLength != expectedLength) {
            return false;
        }
        *reason = get16(payload + 12);
        reserved = get16(payload + 14);
    }

    return reserved == 0 && hostStatusReasonValid(*operation, *status, *reason);
}

static bool capabilityFactsValid(const SS_RUNTIME_SETTINGS_STATE* state,
                                 const uint8_t* payload,
                                 uint16_t status,
                                 uint16_t reason) {
    uint32_t supportedOperations = get32(payload + 12);
    uint32_t proofGatedOperations = get32(payload + 16);
    uint32_t minBitrateKbps = get32(payload + 20);
    uint32_t maxBitrateKbps = get32(payload + 24);
    uint32_t maxFps = get32(payload + 28);
    uint32_t launchBitrateKbps = get32(payload + 32);
    uint32_t launchFps = get32(payload + 36);
    uint32_t launchWidth = get32(payload + 40);
    uint32_t launchHeight = get32(payload + 44);
    uint32_t currentBitrateKbps = get32(payload + 48);
    uint32_t currentFps = get32(payload + 52);
    uint32_t currentWidth = get32(payload + 56);
    uint32_t currentHeight = get32(payload + 60);
    bool unavailable = status != SS_RUNTIME_SETTINGS_STATUS_APPLIED ||
                       reason != SS_RUNTIME_SETTINGS_REASON_NONE;

    if (((supportedOperations | proofGatedOperations) & ~SS_RUNTIME_SETTINGS_MUTATION_OPERATION_MASK) != 0 ||
            (supportedOperations & proofGatedOperations) != 0) {
        return false;
    }
    if (unavailable && (supportedOperations != 0 || proofGatedOperations != 0 ||
                        minBitrateKbps != 0 || maxBitrateKbps != 0 ||
                        maxFps != launchFps)) {
        return false;
    }
    if (launchBitrateKbps == 0 || launchFps == 0 || launchWidth == 0 || launchHeight == 0 ||
            (launchWidth % 2) != 0 || (launchHeight % 2) != 0 ||
            currentBitrateKbps == 0 || currentFps == 0 || currentFps > launchFps ||
            !resolutionWithinLaunch(launchWidth, launchHeight, currentWidth, currentHeight)) {
        return false;
    }
    if ((supportedOperations & (1u << SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS)) != 0 &&
            (minBitrateKbps == 0 || maxBitrateKbps < minBitrateKbps ||
             launchBitrateKbps < minBitrateKbps || launchBitrateKbps > maxBitrateKbps ||
             currentBitrateKbps < minBitrateKbps || currentBitrateKbps > maxBitrateKbps)) {
        return false;
    }
    if ((supportedOperations & (1u << SS_RUNTIME_SETTINGS_OPERATION_SET_FPS)) != 0 &&
            (maxFps == 0 || launchFps > maxFps || currentFps > maxFps)) {
        return false;
    }
    if (state->snapshot.capabilityReceived &&
            (launchBitrateKbps != state->snapshot.launchBitrateKbps ||
             launchFps != state->snapshot.launchFps ||
             launchWidth != state->snapshot.launchWidth ||
             launchHeight != state->snapshot.launchHeight)) {
        return false;
    }
    return true;
}

static void recordCapability(SS_RUNTIME_SETTINGS_STATE* state,
                             const SS_RUNTIME_SETTINGS_COMMAND* query,
                             const uint8_t* payload,
                             uint16_t status,
                             uint16_t reason) {
    bool available = status == SS_RUNTIME_SETTINGS_STATUS_APPLIED &&
                     reason == SS_RUNTIME_SETTINGS_REASON_NONE;
    bool firstCapability = !state->snapshot.capabilityReceived;
    bool applyCurrent = !query->mutationWasActiveAtAcceptance &&
                        query->mutationEpochAtAcceptance == state->mutationEpoch;

    state->snapshot.capabilityReceived = 1;
    state->snapshot.capabilityStatus = status;
    state->snapshot.capabilityReason = reason;
    state->snapshot.supportedOperations = available ? get32(payload + 12) : 0;
    state->snapshot.proofGatedOperations = available ? get32(payload + 16) : 0;
    state->snapshot.minBitrateKbps = available ? get32(payload + 20) : 0;
    state->snapshot.maxBitrateKbps = available ? get32(payload + 24) : 0;
    state->snapshot.maxFps = get32(payload + 28);
    if (firstCapability) {
        state->snapshot.launchBitrateKbps = get32(payload + 32);
        state->snapshot.launchFps = get32(payload + 36);
        state->snapshot.launchWidth = get32(payload + 40);
        state->snapshot.launchHeight = get32(payload + 44);
    }
    if (applyCurrent) {
        state->snapshot.currentBitrateKbps = get32(payload + 48);
        state->snapshot.currentFps = get32(payload + 52);
        state->snapshot.currentWidth = get32(payload + 56);
        state->snapshot.currentHeight = get32(payload + 60);
        state->snapshot.reconciliationRequired = 0;
    }
    else {
        state->snapshot.reconciliationRequired = 1;
    }
}

static void recordMutationCurrent(SS_RUNTIME_SETTINGS_STATE* state,
                                  uint16_t operation,
                                  uint32_t value,
                                  uint32_t secondaryValue) {
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS) {
        state->snapshot.currentBitrateKbps = value;
    }
    else if (operation == SS_RUNTIME_SETTINGS_OPERATION_SET_FPS) {
        state->snapshot.currentFps = value;
    }
    else {
        state->snapshot.currentWidth = value;
        state->snapshot.currentHeight = secondaryValue;
    }
}

int SsRuntimeSettingsProcessAck(SS_RUNTIME_SETTINGS_STATE* state,
                                const uint8_t* payload,
                                size_t payloadLength) {
    SS_RUNTIME_SETTINGS_COMMAND* command;
    SS_RUNTIME_SETTINGS_COMMAND commandSnapshot;
    uint32_t requestId;
    uint16_t operation;
    uint16_t status;
    uint16_t reason;
    uint32_t value;
    uint32_t secondaryValue;

    if (!decodeAckHeader(payload, payloadLength, &requestId, &operation, &status, &reason)) {
        return LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK;
    }

    command = commandForOperation(state, operation);
    if (!command->active || command->requestId != requestId || command->operation != operation) {
        state->snapshot.staleAckCount++;
        changed(state);
        return LI_RUNTIME_SETTINGS_ERROR_STALE_ACK;
    }

    commandSnapshot = *command;
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        if (!capabilityFactsValid(state, payload, status, reason)) {
            return LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK;
        }
    }
    else {
        value = get32(payload + 8);
        secondaryValue = operation == SS_RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION ?
            get32(payload + 12) : 0;
        if (!currentValueSane(&state->snapshot, command, operation, value, secondaryValue)) {
            return LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK;
        }
        if (status == SS_RUNTIME_SETTINGS_STATUS_APPLIED &&
                (value != command->requestedValue ||
                 secondaryValue != command->requestedSecondaryValue)) {
            return LI_RUNTIME_SETTINGS_ERROR_MALFORMED_ACK;
        }
    }

    command->active = false;
    if (operation == SS_RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES) {
        recordCapability(state, &commandSnapshot, payload, status, reason);
    }
    else {
        recordMutationCurrent(state, operation, value, secondaryValue);
    }

    publishCommand(state, operation, requestId,
                   status == SS_RUNTIME_SETTINGS_STATUS_APPLIED ?
                       SS_RUNTIME_SETTINGS_OUTCOME_APPLIED : SS_RUNTIME_SETTINGS_OUTCOME_REJECTED,
                   status, reason);
    return 0;
}

static void timeoutCommand(SS_RUNTIME_SETTINGS_STATE* state,
                           SS_RUNTIME_SETTINGS_COMMAND* command,
                           uint64_t nowMs) {
    uint64_t elapsed = nowMs >= command->sentAtMs ? nowMs - command->sentAtMs : 0;
    if (command->active && elapsed >= SS_RUNTIME_SETTINGS_TIMEOUT_MS) {
        uint32_t requestId = command->requestId;
        uint16_t operation = command->operation;
        command->active = false;
        publishCommand(state, operation, requestId, SS_RUNTIME_SETTINGS_OUTCOME_TIMED_OUT,
                       SS_RUNTIME_SETTINGS_STATUS_FAILED,
                       SS_RUNTIME_SETTINGS_REASON_NO_ACK);
    }
}

void SsRuntimeSettingsCheckTimeouts(SS_RUNTIME_SETTINGS_STATE* state, uint64_t nowMs) {
    timeoutCommand(state, &state->query, nowMs);
    timeoutCommand(state, &state->mutation, nowMs);
}

static uint32_t commandRemainingMs(const SS_RUNTIME_SETTINGS_COMMAND* command,
                                   uint64_t nowMs) {
    uint64_t elapsed;
    if (!command->active) {
        return UINT_MAX;
    }
    elapsed = nowMs >= command->sentAtMs ? nowMs - command->sentAtMs : 0;
    if (elapsed >= SS_RUNTIME_SETTINGS_TIMEOUT_MS) {
        return 0;
    }
    return (uint32_t)(SS_RUNTIME_SETTINGS_TIMEOUT_MS - elapsed);
}

uint32_t SsRuntimeSettingsNextTimeoutMs(const SS_RUNTIME_SETTINGS_STATE* state,
                                        uint64_t nowMs,
                                        uint32_t defaultWaitMs) {
    uint32_t query = commandRemainingMs(&state->query, nowMs);
    uint32_t mutation = commandRemainingMs(&state->mutation, nowMs);
    uint32_t remaining = query < mutation ? query : mutation;
    return remaining == UINT_MAX || remaining > defaultWaitMs ? defaultWaitMs : remaining;
}

static void endCommand(SS_RUNTIME_SETTINGS_STATE* state, SS_RUNTIME_SETTINGS_COMMAND* command) {
    if (command->active) {
        uint32_t requestId = command->requestId;
        uint16_t operation = command->operation;
        command->active = false;
        publishCommand(state, operation, requestId, SS_RUNTIME_SETTINGS_OUTCOME_STREAM_ENDED,
                       SS_RUNTIME_SETTINGS_STATUS_FAILED,
                       SS_RUNTIME_SETTINGS_REASON_STREAM_ENDED);
    }
}

void SsRuntimeSettingsRecordStreamEnd(SS_RUNTIME_SETTINGS_STATE* state) {
    if (state->sessionEnded) {
        return;
    }
    state->sessionEnded = true;
    state->snapshot.sessionActive = 0;
    changed(state);
    endCommand(state, &state->query);
    endCommand(state, &state->mutation);
}

void SsRuntimeSettingsGetSnapshot(const SS_RUNTIME_SETTINGS_STATE* state,
                                  SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot) {
    *snapshot = state->snapshot;
}

void SsRuntimeSettingsSnapshotToWire(
    const SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot,
    uint64_t output[SS_RUNTIME_SETTINGS_SNAPSHOT_WIRE_LENGTH]) {
    output[0] = snapshot->version;
    output[1] = snapshot->generation;
    output[2] = snapshot->sessionEpoch;
    output[3] = snapshot->sessionActive;
    output[4] = snapshot->capabilityReceived;
    output[5] = snapshot->capabilityStatus;
    output[6] = snapshot->capabilityReason;
    output[7] = snapshot->supportedOperations;
    output[8] = snapshot->proofGatedOperations;
    output[9] = snapshot->minBitrateKbps;
    output[10] = snapshot->maxBitrateKbps;
    output[11] = snapshot->maxFps;
    output[12] = snapshot->launchBitrateKbps;
    output[13] = snapshot->launchFps;
    output[14] = snapshot->launchWidth;
    output[15] = snapshot->launchHeight;
    output[16] = snapshot->currentBitrateKbps;
    output[17] = snapshot->currentFps;
    output[18] = snapshot->currentWidth;
    output[19] = snapshot->currentHeight;
    output[20] = snapshot->queryRequestId;
    output[21] = snapshot->queryOutcome;
    output[22] = snapshot->queryStatus;
    output[23] = snapshot->queryReason;
    output[24] = snapshot->mutationRequestId;
    output[25] = snapshot->mutationOperation;
    output[26] = snapshot->mutationOutcome;
    output[27] = snapshot->mutationStatus;
    output[28] = snapshot->mutationReason;
    output[29] = snapshot->staleAckCount;
    output[30] = snapshot->reconciliationRequired;
}
