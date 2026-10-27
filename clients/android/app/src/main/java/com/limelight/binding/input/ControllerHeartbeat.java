package com.limelight.binding.input;

final class ControllerHeartbeat implements Runnable {
    static final long INTERVAL_MS = 250;
    private static final int DISCONNECT_REPEATS = 4;
    private static final int MAX_CONTROLLERS = 16;

    interface Scheduler {
        void postDelayed(Runnable action, long delayMs);
        void remove(Runnable action);
    }

    interface Sender {
        void send(State state);
    }

    static final class State {
        final short controllerNumber;
        final short activeMask;
        final int buttons;
        final byte leftTrigger;
        final byte rightTrigger;
        final short leftStickX;
        final short leftStickY;
        final short rightStickX;
        final short rightStickY;

        State(short controllerNumber, short activeMask, int buttons,
              byte leftTrigger, byte rightTrigger,
              short leftStickX, short leftStickY,
              short rightStickX, short rightStickY) {
            this.controllerNumber = controllerNumber;
            this.activeMask = activeMask;
            this.buttons = buttons;
            this.leftTrigger = leftTrigger;
            this.rightTrigger = rightTrigger;
            this.leftStickX = leftStickX;
            this.leftStickY = leftStickY;
            this.rightStickX = rightStickX;
            this.rightStickY = rightStickY;
        }
    }

    private final Scheduler scheduler;
    private final Sender sender;
    private final State[] states = new State[MAX_CONTROLLERS];
    private final int[] disconnectRepeats = new int[MAX_CONTROLLERS];
    private boolean started;
    private boolean stopped;

    ControllerHeartbeat(Scheduler scheduler, Sender sender) {
        this.scheduler = scheduler;
        this.sender = sender;
    }

    synchronized void start() {
        if (started || stopped) return;
        started = true;
        scheduler.postDelayed(this, INTERVAL_MS);
    }

    synchronized void record(State state) {
        int controller = state.controllerNumber;
        if (stopped || controller < 0 || controller >= MAX_CONTROLLERS) return;
        states[controller] = state;
        disconnectRepeats[controller] = isActive(state) ? 0 : DISCONNECT_REPEATS;
    }

    @Override
    public synchronized void run() {
        if (stopped) return;
        for (int controller = 0; controller < states.length; controller++) {
            State state = states[controller];
            if (state == null) continue;
            if (isActive(state)) {
                sender.send(state);
            }
            else if (disconnectRepeats[controller] > 0) {
                sender.send(state);
                disconnectRepeats[controller]--;
                if (disconnectRepeats[controller] == 0) states[controller] = null;
            }
        }
        scheduler.postDelayed(this, INTERVAL_MS);
    }

    synchronized void close() {
        if (stopped) return;
        stopped = true;
        scheduler.remove(this);
        for (int controller = 0; controller < states.length; controller++) {
            states[controller] = null;
            disconnectRepeats[controller] = 0;
        }
    }

    private static boolean isActive(State state) {
        return (state.activeMask & (1 << state.controllerNumber)) != 0;
    }
}
