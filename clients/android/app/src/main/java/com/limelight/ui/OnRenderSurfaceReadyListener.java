package com.limelight.ui;

import android.view.Surface;

/**
 * Signals that a GL render mode has produced the {@link Surface} the decoder
 * should output into.
 *
 * GL render modes do not decode into the window surface directly: they hand
 * MediaCodec a SurfaceTexture-backed surface, process each frame, then draw to
 * the display. The container therefore cannot start the stream until the
 * renderer reports its surface here.
 *
 * Owned by the container rather than by any one renderer, so render modes stay
 * independent of each other.
 */
public interface OnRenderSurfaceReadyListener {
    void onRenderSurfaceReady(Surface surface);
}
