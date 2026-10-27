package com.limelight.binding.input;

import org.junit.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;

public class ControllerHeartbeatTest {
    private static final class FakeScheduler implements ControllerHeartbeat.Scheduler {
        Runnable scheduled;
        long delayMs;

        @Override
        public void postDelayed(Runnable action, long delayMs) {
            this.scheduled = action;
            this.delayMs = delayMs;
        }

        @Override
        public void remove(Runnable action) {
            if (scheduled == action) scheduled = null;
        }

        void tick() {
            Runnable action = scheduled;
            scheduled = null;
            action.run();
        }
    }

    private static ControllerHeartbeat.State state(short controller, short activeMask, int buttons) {
        return new ControllerHeartbeat.State(controller, activeMask, buttons,
                (byte) 1, (byte) 2,
                (short) 3, (short) 4, (short) 5, (short) 6);
    }

    @Test
    public void activeStateRepeatsAtTheFixedInterval() {
        FakeScheduler scheduler = new FakeScheduler();
        List<ControllerHeartbeat.State> sent = new ArrayList<>();
        ControllerHeartbeat heartbeat = new ControllerHeartbeat(scheduler, sent::add);
        ControllerHeartbeat.State held = state((short) 0, (short) 1, 0x1000);

        heartbeat.start();
        heartbeat.record(held);
        assertEquals(ControllerHeartbeat.INTERVAL_MS, scheduler.delayMs);
        assertSame(heartbeat, scheduler.scheduled);

        scheduler.tick();
        assertEquals(1, sent.size());
        assertSame(held, sent.get(0));
        assertSame(heartbeat, scheduler.scheduled);
    }

    @Test
    public void eachHeartbeatUsesTheLatestControllerState() {
        FakeScheduler scheduler = new FakeScheduler();
        List<ControllerHeartbeat.State> sent = new ArrayList<>();
        ControllerHeartbeat heartbeat = new ControllerHeartbeat(scheduler, sent::add);
        ControllerHeartbeat.State first = state((short) 0, (short) 1, 0x1000);
        ControllerHeartbeat.State latest = state((short) 0, (short) 1, 0);

        heartbeat.start();
        heartbeat.record(first);
        heartbeat.record(latest);
        scheduler.tick();

        assertEquals(1, sent.size());
        assertSame(latest, sent.get(0));
    }

    @Test
    public void disconnectStateRepeatsFourTimesThenStops() {
        FakeScheduler scheduler = new FakeScheduler();
        List<ControllerHeartbeat.State> sent = new ArrayList<>();
        ControllerHeartbeat heartbeat = new ControllerHeartbeat(scheduler, sent::add);
        ControllerHeartbeat.State disconnected = state((short) 0, (short) 0, 0);

        heartbeat.start();
        heartbeat.record(disconnected);
        for (int index = 0; index < 6; index++) scheduler.tick();

        assertEquals(4, sent.size());
    }

    @Test
    public void closeRemovesTheScheduledHeartbeat() {
        FakeScheduler scheduler = new FakeScheduler();
        List<ControllerHeartbeat.State> sent = new ArrayList<>();
        ControllerHeartbeat heartbeat = new ControllerHeartbeat(scheduler, sent::add);

        heartbeat.start();
        heartbeat.record(state((short) 0, (short) 1, 0x1000));
        heartbeat.close();

        assertNull(scheduler.scheduled);
        heartbeat.run();
        assertEquals(0, sent.size());
    }
}
