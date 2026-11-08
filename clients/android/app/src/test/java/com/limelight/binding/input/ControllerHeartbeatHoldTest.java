package com.limelight.binding.input;

import org.junit.Test;
import java.util.ArrayList;
import java.util.List;
import static org.junit.Assert.*;

public class ControllerHeartbeatHoldTest {
    @Test public void divertedReleasesCannotReplayHeldGameplayAfterResume() {
        List<Runnable> tasks = new ArrayList<>();
        List<ControllerHeartbeat.State> sent = new ArrayList<>();
        ControllerHeartbeat heartbeat = new ControllerHeartbeat(new ControllerHeartbeat.Scheduler() {
            @Override public void postDelayed(Runnable action, long delay) { tasks.add(action); }
            @Override public void remove(Runnable action) { tasks.remove(action); }
        }, sent::add);
        heartbeat.start();
        for (int cycle = 0; cycle < 5; cycle++) {
            heartbeat.record(new ControllerHeartbeat.State((short)0, (short)1, 0x1000,
                    (byte)10, (byte)20, (short)100, (short)200, (short)300, (short)400));
            heartbeat.neutralize();
            tasks.remove(0).run();
            ControllerHeartbeat.State afterHold = sent.get(sent.size() - 1);
            assertEquals(1, afterHold.activeMask);
            assertEquals(0, afterHold.buttons);
            assertEquals(0, afterHold.leftTrigger);
            assertEquals(0, afterHold.rightTrigger);
            assertEquals(0, afterHold.leftStickX);
            assertEquals(0, afterHold.leftStickY);
            assertEquals(0, afterHold.rightStickX);
            assertEquals(0, afterHold.rightStickY);
            // No new controller event: heartbeat must stay neutral after Resume.
            tasks.remove(0).run();
            assertEquals(0, sent.get(sent.size() - 1).buttons);
        }
        heartbeat.close();
        assertTrue(tasks.isEmpty());
    }
}
