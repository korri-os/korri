#pragma once

#include <stddef.h>

#include "Limelight.h"

#define SS_RUNTIME_SETTINGS_REQUEST_PACKET 0x5504
#define SS_RUNTIME_SETTINGS_ACK_PACKET 0x5505
#define SS_RUNTIME_SETTINGS_TIMEOUT_MS 3000
#define SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES 16

typedef struct _SS_RUNTIME_SETTINGS_COMMAND {
    bool active;
    uint32_t requestId;
    uint16_t operation;
    uint32_t requestedValue;
    uint32_t requestedSecondaryValue;
    uint64_t sentAtMs;
    uint64_t mutationEpochAtAcceptance;
    bool mutationWasActiveAtAcceptance;
    uint32_t minBitrateKbpsAtAcceptance;
    uint32_t maxBitrateKbpsAtAcceptance;
} SS_RUNTIME_SETTINGS_COMMAND;

typedef struct _SS_RUNTIME_SETTINGS_STATE {
    SS_RUNTIME_SETTINGS_SNAPSHOT snapshot;
    SS_RUNTIME_SETTINGS_COMMAND query;
    SS_RUNTIME_SETTINGS_COMMAND mutation;
    uint32_t lastIssuedRequestId;
    uint64_t mutationEpoch;
    bool sessionEnded;
} SS_RUNTIME_SETTINGS_STATE;

void SsRuntimeSettingsInitialize(SS_RUNTIME_SETTINGS_STATE* state);
void SsRuntimeSettingsBeginSession(SS_RUNTIME_SETTINGS_STATE* state, uint64_t sessionEpoch);
void SsRuntimeSettingsSetSessionActive(SS_RUNTIME_SETTINGS_STATE* state, bool active);
int SsRuntimeSettingsPrepareRequest(SS_RUNTIME_SETTINGS_STATE* state,
                                    uint64_t expectedSessionEpoch,
                                    uint64_t nowMs,
                                    uint32_t requestId,
                                    uint16_t operation,
                                    uint32_t value,
                                    uint32_t secondaryValue,
                                    uint8_t output[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES],
                                    size_t* outputLength);
void SsRuntimeSettingsRecordSendFailure(SS_RUNTIME_SETTINGS_STATE* state,
                                        uint32_t requestId,
                                        uint16_t operation);
int SsRuntimeSettingsProcessAck(SS_RUNTIME_SETTINGS_STATE* state,
                                const uint8_t* payload,
                                size_t payloadLength);
void SsRuntimeSettingsCheckTimeouts(SS_RUNTIME_SETTINGS_STATE* state, uint64_t nowMs);
uint32_t SsRuntimeSettingsNextTimeoutMs(const SS_RUNTIME_SETTINGS_STATE* state,
                                        uint64_t nowMs,
                                        uint32_t defaultWaitMs);
void SsRuntimeSettingsRecordStreamEnd(SS_RUNTIME_SETTINGS_STATE* state);
void SsRuntimeSettingsGetSnapshot(const SS_RUNTIME_SETTINGS_STATE* state,
                                  SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot);
void SsRuntimeSettingsSnapshotToWire(
    const SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot,
    uint64_t output[SS_RUNTIME_SETTINGS_SNAPSHOT_WIRE_LENGTH]);
