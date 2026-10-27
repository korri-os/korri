package com.limelight;

import com.limelight.nvstream.jni.MoonBridge;

import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;

/** Pins the hand-mirrored v4 session lifecycle treaty mapping. */
@RunWith(RobolectricTestRunner.class)
public class KorriSessionOverlayTest {

    @Test
    public void semanticStagesFollowNativeMoonlightOrder() {
        assertEquals("launching-app", KorriSessionOverlay.semanticStage("Korri Stream", "Korri Stream"));
        assertEquals("initializing", KorriSessionOverlay.semanticStage("platform initialization", "Korri Stream"));
        assertEquals("initializing", KorriSessionOverlay.semanticStage("name resolution", "Korri Stream"));
        assertEquals("initializing", KorriSessionOverlay.semanticStage("audio stream initialization", "Korri Stream"));
        assertEquals("handshaking", KorriSessionOverlay.semanticStage("RTSP handshake", "Korri Stream"));
        assertEquals("establishing-streams", KorriSessionOverlay.semanticStage("control stream establishment", "Korri Stream"));
    }

    @Test
    public void failuresAndTerminationsMapToTreatyReasons() {
        assertEquals("HostUnreachable", KorriSessionOverlay.failureReason("name resolution", "Korri Stream", -408, false));
        assertEquals("PermissionDenied", KorriSessionOverlay.failureReason("RTSP handshake", "Korri Stream", 403, false));
        assertEquals("AppLaunchFailed", KorriSessionOverlay.failureReason("Korri Stream", "Korri Stream", 1, false));
        assertEquals("DecoderInitFailed", KorriSessionOverlay.failureReason("video stream initialization", "Korri Stream", 1, false));
        assertEquals("NoVideoTraffic", KorriSessionOverlay.terminationReason(MoonBridge.ML_ERROR_NO_VIDEO_TRAFFIC));
        assertEquals("ConnectionLost", KorriSessionOverlay.terminationReason(-999));
    }

    @Test
    public void eventFactoriesEmitTreatyJson() throws Exception {
        JSONObject starting = KorriSessionOverlay.stageStartingEvent("RTSP handshake", "Korri Stream");
        assertEquals("stage-starting", starting.getString("type"));
        assertEquals("handshaking", starting.getString("stage"));

        JSONObject failed = KorriSessionOverlay.failedEvent("video stream initialization", "Korri Stream", 7, false);
        assertEquals("failed", failed.getString("type"));
        assertEquals("DecoderInitFailed", failed.getString("reason"));
        assertEquals(7, failed.getInt("errorCode"));

        JSONObject terminated = KorriSessionOverlay.terminatedEvent(false, MoonBridge.ML_ERROR_NO_VIDEO_FRAME);
        assertFalse(terminated.getBoolean("graceful"));
        assertEquals("NoVideoTraffic", terminated.getString("reason"));
    }

    @Test
    public void preStreamLifecycleEventsKeepTheirTreatyBytes() {
        assertEquals(
                "{\"type\":\"stage-starting\",\"stage\":\"initializing\",\"detail\":\"name resolution\"}",
                KorriSessionOverlay.stageStartingEvent("name resolution", "Korri Stream").toString());
        assertEquals(
                "{\"type\":\"stage-complete\",\"stage\":\"handshaking\",\"detail\":\"RTSP handshake\"}",
                KorriSessionOverlay.stageCompleteEvent("RTSP handshake", "Korri Stream").toString());
        assertEquals(
                "{\"type\":\"failed\",\"reason\":\"HostUnreachable\",\"stage\":\"initializing\",\"errorCode\":-408,\"detail\":\"name resolution\"}",
                KorriSessionOverlay.failedEvent("name resolution", "Korri Stream", -408, false).toString());
        assertEquals("{\"type\":\"connected\"}", KorriSessionOverlay.connectedEvent().toString());
        assertEquals(
                "{\"type\":\"terminated\",\"graceful\":false,\"reason\":\"ConnectionLost\",\"errorCode\":-999}",
                KorriSessionOverlay.terminatedEvent(false, -999).toString());
        assertEquals(
                "{\"type\":\"terminated\",\"graceful\":true,\"reason\":\"Unknown\",\"errorCode\":0}",
                KorriSessionOverlay.terminatedEvent(true, 0).toString());
    }

    @Test
    public void decoderUnsupportedEventIsAWebLifecycleFailure() throws Exception {
        JSONObject event = KorriSessionOverlay.decoderUnsupportedEvent();
        assertEquals("failed", event.getString("type"));
        assertEquals("DecoderInitFailed", event.getString("reason"));
        assertEquals("initializing", event.getString("stage"));
    }
}
