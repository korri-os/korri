package com.limelight.nvstream;

import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.shadows.ShadowMoonBridge;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import java.util.ArrayList;
import java.util.List;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28, shadows = NvConnectionInputHoldTest.MotionCalls.class)
public class NvConnectionInputHoldTest {
    private static final class Motion {
        final byte controller;
        final byte type;
        final float x, y, z;
        Motion(byte controller, byte type, float x, float y, float z) {
            this.controller = controller;
            this.type = type;
            this.x = x;
            this.y = y;
            this.z = z;
        }
    }

    /** Records the actual JNI motion-call boundary; no native library or stream starts. */
    @Implements(value = MoonBridge.class, isInAndroidSdk = false)
    public static class MotionCalls extends ShadowMoonBridge {
        static final List<Motion> calls = new ArrayList<>();
        @Implementation protected static int sendControllerMotionEvent(byte controller, byte type,
                float x, float y, float z) {
            calls.add(new Motion(controller, type, x, y, z));
            return 0;
        }
    }

    @Before public void clearCalls() { MotionCalls.calls.clear(); }

    private NvConnection connection() {
        return new NvConnection(RuntimeEnvironment.getApplication(),
                new ComputerDetails.AddressTuple("127.0.0.1", 47989), 47984, "test",
                null, null, null, NvConnection.AppLaunchPolicy.EXACT);
    }

    private void assertMotion(int index, byte controller, byte type, float x, float y, float z) {
        Motion call = MotionCalls.calls.get(index);
        assertEquals(controller, call.controller);
        assertEquals(type, call.type);
        assertEquals(x, call.x, 0);
        assertEquals(y, call.y, 0);
        assertEquals(z, call.z, 0);
    }

    @Test public void holdSendsReliableZeroGyroBlocksLaterSamplesAndResumeAcceptsFreshRates() {
        NvConnection connection = connection();
        byte controller = 3;
        assertEquals(0, connection.sendControllerMotionEvent(controller, MoonBridge.LI_MOTION_TYPE_GYRO, 1, -2, 3));
        assertMotion(0, controller, MoonBridge.LI_MOTION_TYPE_GYRO, 1, -2, 3);
        connection.setInputHeld(true);
        assertEquals(2, MotionCalls.calls.size());
        assertMotion(1, controller, MoonBridge.LI_MOTION_TYPE_GYRO, 0, 0, 0);
        assertEquals(MoonBridge.LI_ERR_UNSUPPORTED,
                connection.sendControllerMotionEvent(controller, MoonBridge.LI_MOTION_TYPE_GYRO, 7, 8, 9));
        assertEquals(MoonBridge.LI_ERR_UNSUPPORTED,
                connection.sendControllerMotionEvent(controller, MoonBridge.LI_MOTION_TYPE_ACCEL, 1, 2, 9));
        connection.setInputHeld(true);
        assertEquals(2, MotionCalls.calls.size());
        connection.setInputHeld(false);
        assertEquals("Resume must not restore a stale gyro rate", 2, MotionCalls.calls.size());
        assertEquals(0, connection.sendControllerMotionEvent(controller, MoonBridge.LI_MOTION_TYPE_GYRO, -4, 5, 6));
        assertMotion(2, controller, MoonBridge.LI_MOTION_TYPE_GYRO, -4, 5, 6);
        connection.setInputHeld(true);
        assertMotion(3, controller, MoonBridge.LI_MOTION_TYPE_GYRO, 0, 0, 0);
        assertEquals(4, MotionCalls.calls.size());
    }

    @Test public void holdZerosEveryGyroControllerButLeavesAccelerationAndAttitudeUnchanged() {
        NvConnection connection = connection();
        connection.sendControllerMotionEvent((byte) 1, MoonBridge.LI_MOTION_TYPE_GYRO, 1, 2, 3);
        connection.sendControllerMotionEvent((byte) 4, MoonBridge.LI_MOTION_TYPE_GYRO, 4, 5, 6);
        connection.sendControllerMotionEvent((byte) 7, MoonBridge.LI_MOTION_TYPE_ACCEL, 1, 2, 9);
        connection.setInputHeld(true);
        assertEquals(5, MotionCalls.calls.size());
        java.util.Set<Byte> zeroed = new java.util.HashSet<>();
        for (Motion call : MotionCalls.calls.subList(3, 5)) {
            assertEquals(MoonBridge.LI_MOTION_TYPE_GYRO, call.type);
            assertEquals(0, call.x, 0);
            assertEquals(0, call.y, 0);
            assertEquals(0, call.z, 0);
            zeroed.add(call.controller);
        }
        assertEquals(new java.util.HashSet<>(java.util.Arrays.asList((byte) 1, (byte) 4)), zeroed);
        assertMotion(2, (byte) 7, MoonBridge.LI_MOTION_TYPE_ACCEL, 1, 2, 9);
        connection.setInputHeld(false);
        connection.sendControllerMotionEvent((byte) 7, MoonBridge.LI_MOTION_TYPE_ACCEL, 2, 3, 8);
        connection.setInputHeld(true);
        assertEquals("No fresh gyro sample: acceleration alone must not trigger neutralization", 6,
                MotionCalls.calls.size());
    }
}
