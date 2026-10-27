#pragma once

#include "SunshineRuntimeSettings.h"

typedef void (*SS_RUNTIME_SETTINGS_LOCK_CALLBACK)(void* context);
typedef int (*SS_RUNTIME_SETTINGS_READY_CALLBACK)(void* context);
typedef bool (*SS_RUNTIME_SETTINGS_SEND_CALLBACK)(void* context,
                                                   const uint8_t* payload,
                                                   size_t payloadLength);
typedef uint64_t (*SS_RUNTIME_SETTINGS_CLOCK_CALLBACK)(void* context);

typedef struct _SS_RUNTIME_SETTINGS_DISPATCH {
    SS_RUNTIME_SETTINGS_STATE* state;
    void* context;
    SS_RUNTIME_SETTINGS_LOCK_CALLBACK lock;
    SS_RUNTIME_SETTINGS_LOCK_CALLBACK unlock;
    SS_RUNTIME_SETTINGS_READY_CALLBACK ready;
    SS_RUNTIME_SETTINGS_SEND_CALLBACK send;
    SS_RUNTIME_SETTINGS_CLOCK_CALLBACK clock;
    int inactiveError;
} SS_RUNTIME_SETTINGS_DISPATCH;

void SsRuntimeSettingsDispatchInitialize(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                         SS_RUNTIME_SETTINGS_STATE* state,
                                         void* context,
                                         SS_RUNTIME_SETTINGS_LOCK_CALLBACK lock,
                                         SS_RUNTIME_SETTINGS_LOCK_CALLBACK unlock,
                                         SS_RUNTIME_SETTINGS_READY_CALLBACK ready,
                                         SS_RUNTIME_SETTINGS_SEND_CALLBACK send,
                                         SS_RUNTIME_SETTINGS_CLOCK_CALLBACK clock);
void SsRuntimeSettingsDispatchBeginSession(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                           uint64_t sessionEpoch);
void SsRuntimeSettingsDispatchSetActive(SS_RUNTIME_SETTINGS_DISPATCH* dispatch, bool active);
int SsRuntimeSettingsDispatchRequest(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                     uint64_t expectedSessionEpoch,
                                     uint32_t requestId,
                                     uint16_t operation,
                                     uint32_t value,
                                     uint32_t secondaryValue);
void SsRuntimeSettingsDispatchEndSession(SS_RUNTIME_SETTINGS_DISPATCH* dispatch);
void SsRuntimeSettingsDispatchGetSnapshot(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                          SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot);
