#include "SunshineRuntimeSettingsDispatch.h"

void SsRuntimeSettingsDispatchInitialize(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                         SS_RUNTIME_SETTINGS_STATE* state,
                                         void* context,
                                         SS_RUNTIME_SETTINGS_LOCK_CALLBACK lock,
                                         SS_RUNTIME_SETTINGS_LOCK_CALLBACK unlock,
                                         SS_RUNTIME_SETTINGS_READY_CALLBACK ready,
                                         SS_RUNTIME_SETTINGS_SEND_CALLBACK send,
                                         SS_RUNTIME_SETTINGS_CLOCK_CALLBACK clock) {
    dispatch->state = state;
    dispatch->context = context;
    dispatch->lock = lock;
    dispatch->unlock = unlock;
    dispatch->ready = ready;
    dispatch->send = send;
    dispatch->clock = clock;
    dispatch->inactiveError = LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
}

void SsRuntimeSettingsDispatchBeginSession(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                           uint64_t sessionEpoch) {
    dispatch->lock(dispatch->context);
    SsRuntimeSettingsBeginSession(dispatch->state, sessionEpoch);
    dispatch->inactiveError = LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
    dispatch->unlock(dispatch->context);
}

void SsRuntimeSettingsDispatchSetActive(SS_RUNTIME_SETTINGS_DISPATCH* dispatch, bool active) {
    int readinessError = LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;

    dispatch->lock(dispatch->context);
    if (active) {
        readinessError = dispatch->ready == NULL ? 0 : dispatch->ready(dispatch->context);
    }
    SsRuntimeSettingsSetSessionActive(dispatch->state, active && readinessError == 0);
    dispatch->inactiveError = active && readinessError != 0 ?
        readinessError : LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
    dispatch->unlock(dispatch->context);
}

int SsRuntimeSettingsDispatchRequest(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                     uint64_t expectedSessionEpoch,
                                     uint32_t requestId,
                                     uint16_t operation,
                                     uint32_t value,
                                     uint32_t secondaryValue) {
    uint8_t payload[SS_RUNTIME_SETTINGS_MAX_REQUEST_BYTES];
    size_t payloadLength;
    int result;

    dispatch->lock(dispatch->context);
    if (!dispatch->state->snapshot.sessionActive) {
        result = dispatch->inactiveError;
        dispatch->unlock(dispatch->context);
        return result;
    }
    if (expectedSessionEpoch == 0 ||
            expectedSessionEpoch != dispatch->state->snapshot.sessionEpoch) {
        dispatch->unlock(dispatch->context);
        return LI_RUNTIME_SETTINGS_ERROR_STALE_SESSION;
    }
    if (dispatch->ready != NULL) {
        result = dispatch->ready(dispatch->context);
        if (result != 0) {
            dispatch->unlock(dispatch->context);
            return result;
        }
    }
    result = SsRuntimeSettingsPrepareRequest(dispatch->state,
                                             expectedSessionEpoch,
                                             dispatch->clock(dispatch->context),
                                             requestId,
                                             operation,
                                             value,
                                             secondaryValue,
                                             payload,
                                             &payloadLength);
    if (result == 0 && !dispatch->send(dispatch->context, payload, payloadLength)) {
        SsRuntimeSettingsRecordSendFailure(dispatch->state, requestId, operation);
        result = LI_RUNTIME_SETTINGS_ERROR_SEND_FAILED;
    }
    dispatch->unlock(dispatch->context);
    return result;
}

void SsRuntimeSettingsDispatchEndSession(SS_RUNTIME_SETTINGS_DISPATCH* dispatch) {
    dispatch->lock(dispatch->context);
    SsRuntimeSettingsRecordStreamEnd(dispatch->state);
    dispatch->inactiveError = LI_RUNTIME_SETTINGS_ERROR_CONTROL_NOT_READY;
    dispatch->unlock(dispatch->context);
}

void SsRuntimeSettingsDispatchGetSnapshot(SS_RUNTIME_SETTINGS_DISPATCH* dispatch,
                                          SS_RUNTIME_SETTINGS_SNAPSHOT* snapshot) {
    dispatch->lock(dispatch->context);
    SsRuntimeSettingsGetSnapshot(dispatch->state, snapshot);
    dispatch->unlock(dispatch->context);
}
