package com.limelight;

import org.junit.Test;

import static org.junit.Assert.*;
import static com.limelight.KorriStreamViewerState.Phase.*;

public class KorriStreamViewerStateTest {
    private static final String LAUNCH = "0123456789abcdef0123456789abcdef";

    @Test
    public void startRequiresBothActualConnectionAndWindowFocus() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        long lease = state.attach(LAUNCH, 1);
        assertEquals(STARTING, state.phase());
        state.focus(lease, true);
        assertEquals(STARTING, state.phase());
        assertTrue(state.opening(lease));
        assertEquals(STARTING, state.phase());
        state.connected(lease);
        assertEquals(RUNNING, state.phase());
    }

    @Test
    public void wrongLaunchGenerationAndDuplicateActivityCannotClaimViewer() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 2);
        assertEquals(0, state.attach("ffffffffffffffffffffffffffffffff", 2));
        assertEquals(0, state.attach(LAUNCH, 1));
        assertTrue(state.attach(LAUNCH, 2) > 0);
        assertEquals(0, state.attach(LAUNCH, 2));
    }

    @Test
    public void backgroundWaitsForActivityAndStreamToActuallyClose() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        long lease = running(state);
        state.background();
        assertEquals(BACKGROUNDING, state.phase());
        state.destroyed(lease, false);
        assertEquals(BACKGROUNDING, state.phase());
        state.closed(lease);
        assertEquals(BACKGROUNDED, state.phase());
    }

    @Test
    public void stopWaitsForBothResourcesInEitherCloseOrder() {
        for (boolean activityFirst : new boolean[]{true, false}) {
            KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
            long lease = running(state);
            state.stop();
            if (activityFirst) state.destroyed(lease, false);
            else state.closed(lease);
            assertEquals(STOPPING, state.phase());
            if (activityFirst) state.closed(lease);
            else state.destroyed(lease, false);
            assertEquals(STOPPED, state.phase());
            assertEquals(0, state.attach(LAUNCH, 1));
        }
    }

    @Test
    public void resumeRecreatesOnlyTheSameExactViewer() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 3);
        long old = running(state);
        state.background();
        state.closed(old);
        state.destroyed(old, false);
        state.resume();
        assertEquals(STARTING, state.phase());
        long current = state.attach(LAUNCH, 3);
        assertTrue(current > old);
        assertTrue(state.opening(current));
        state.focus(current, true);
        state.connected(old);
        assertEquals(STARTING, state.phase());
        state.connected(current);
        state.failed(old);
        state.destroyed(old, false);
        state.closed(old);
        assertEquals(RUNNING, state.phase());
    }

    @Test
    public void recreationDrainsOldResourcesWithoutRetargetingNewCallbacks() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 4);
        long old = running(state);
        state.destroyed(old, true);
        assertEquals(STARTING, state.phase());
        long current = state.attach(LAUNCH, 4);
        state.opening(current);
        state.connected(current);
        state.focus(current, true);
        state.closed(old);
        state.focus(old, false);
        state.failed(old);
        assertEquals(RUNNING, state.phase());
        state.stop();
        state.destroyed(current, false);
        assertEquals(STOPPING, state.phase());
        state.closed(current);
        assertEquals(STOPPED, state.phase());
    }

    @Test
    public void failureIsNotSuccessAndStopStillRequiresResourceRelease() throws Exception {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        long lease = running(state);
        state.failed(lease);
        assertEquals(FAILED, state.phase());
        assertFalse(state.await(RUNNING, 1));
        state.stop();
        assertEquals(STOPPING, state.phase());
        assertFalse(state.await(STOPPED, 1));
        state.closed(lease);
        state.destroyed(lease, false);
        assertTrue(state.await(STOPPED, 1));
    }

    @Test
    public void anActivityThatNeverConnectedCanBeStoppedWithoutInventingAStream() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        long lease = state.attach(LAUNCH, 1);
        state.stop();
        assertEquals(STOPPING, state.phase());
        state.destroyed(lease, false);
        assertEquals(STOPPED, state.phase());
    }

    @Test
    public void expectedBackgroundTerminationDoesNotPoisonTheSameViewerResume() {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        long old = running(state);
        state.background();
        assertFalse(state.canPresent(old));
        state.failed(old);
        state.closed(old);
        state.destroyed(old, false);
        state.resume();
        long current = state.attach(LAUNCH, 1);
        assertTrue(current > old);
        state.opening(current);
        state.connected(current);
        state.focus(current, true);
        assertEquals(RUNNING, state.phase());
    }

    @Test
    public void failedActivityStartAndStoppedLaunchCannotReportSuccessfulConnection() throws Exception {
        KorriStreamViewerState state = new KorriStreamViewerState(LAUNCH, 1);
        state.failedToAttach();
        assertFalse(state.await(RUNNING, 1));
        state.stop();
        assertFalse(state.await(RUNNING, 1));
        assertEquals(STOPPED, state.phase());
        assertEquals(0, state.attach(LAUNCH, 1));
    }

    private static long running(KorriStreamViewerState state) {
        long lease = state.attach(LAUNCH, state.generation);
        assertTrue(lease > 0);
        assertTrue(state.opening(lease));
        state.connected(lease);
        state.focus(lease, true);
        assertEquals(RUNNING, state.phase());
        return lease;
    }
}
