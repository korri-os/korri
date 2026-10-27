package com.limelight.korri.overlay;

import java.util.Objects;

/** UI-thread, process-local ownership of the temporary gameplay-overlay hosts. */
public final class KorriOverlayHostExclusion {
    public interface LegacyHost {
        boolean isVisible();
        void closeAndDestroy();
    }

    /** Exact process-local Game/legacy-host generation. */
    public static final class Owner {
        private final LegacyHost host;
        private final long generation;

        private Owner(LegacyHost host, long generation) {
            this.host = host;
            this.generation = generation;
        }

        long generation() {
            return generation;
        }
    }

    private Owner current;
    private long nextGeneration;

    /**
     * Called on Android's UI thread. A visible predecessor is synchronously
     * retired before the replacement generation becomes current.
     */
    public Owner register(LegacyHost host) {
        Objects.requireNonNull(host);
        closeVisibleCurrent();
        Owner owner = new Owner(host, ++nextGeneration);
        current = owner;
        return owner;
    }

    public void unregister(Owner owner) {
        if (isCurrent(owner)) current = null;
    }

    public boolean isCurrent(Owner owner) {
        return owner != null
                && current != null
                && current.host == owner.host
                && current.generation == owner.generation;
    }

    public void globalConnected() {
        closeVisibleCurrent();
    }

    public void openGlobal(Runnable open) {
        Owner predecessor = current;
        if (predecessor == null) {
            open.run();
            return;
        }
        closeVisibleCurrent();
        // A retiring predecessor may unregister itself. It may also install a
        // replacement synchronously; that newer Game generation wins.
        if (current == null || isCurrent(predecessor)) open.run();
    }

    public void openGlobal(Owner owner, Runnable open) {
        if (!isCurrent(owner)) return;
        closeVisibleCurrent();
        // closeAndDestroy is application code and may synchronously install a
        // replacement. Recheck before allowing the displaced Game to open.
        if (isCurrent(owner)) open.run();
    }

    public void hideBoth(Owner owner, Runnable dismissGlobal) {
        if (!isCurrent(owner)) return;
        closeVisibleCurrent();
        if (isCurrent(owner)) dismissGlobal.run();
    }

    private void closeVisibleCurrent() {
        Owner owner = current;
        if (owner != null && owner.host.isVisible()) {
            owner.host.closeAndDestroy();
        }
    }
}
