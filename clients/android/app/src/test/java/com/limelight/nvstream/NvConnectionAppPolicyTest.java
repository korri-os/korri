package com.limelight.nvstream;

import com.limelight.nvstream.http.NvApp;
import org.junit.Test;

import static org.junit.Assert.*;

public class NvConnectionAppPolicyTest {
    @Test
    public void exactViewerCannotResumeAnotherAppThroughAnEmptyOrMatchingUuid() {
        NvApp stream = new NvApp("Korri Stream", "", 2, false);
        assertFalse(NvConnection.matchesRunningApp(NvConnection.AppLaunchPolicy.EXACT, 3, "", stream));
        stream.setAppUUID("stream");
        assertFalse(NvConnection.matchesRunningApp(NvConnection.AppLaunchPolicy.EXACT, 3, "stream", stream));
        assertTrue(NvConnection.matchesRunningApp(NvConnection.AppLaunchPolicy.EXACT, 2, null, stream));
    }

    @Test
    public void exactViewerNeedsAPositiveAppId() {
        assertFalse(NvConnection.matchesRunningApp(NvConnection.AppLaunchPolicy.EXACT, 0, "",
                new NvApp("Korri Stream", "", 0, false)));
    }

    @Test
    public void ordinaryArtemisRetainsItsExplicitUuidRoute() {
        assertTrue(NvConnection.matchesRunningApp(NvConnection.AppLaunchPolicy.REPLACE, 3, "app",
                new NvApp("App", "app", 2, false)));
    }
}
