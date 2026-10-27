package com.limelight;

import org.json.JSONObject;

/** The existing RPC treaty, installed only in the trusted top-level document. */
final class KorriRpcBinding {
    static final String READY_LISTENER = "KorriRpcConsumed";
    private final String origin;
    private final int port;
    private final String capability;
    private boolean portRead;
    private boolean capabilityRead;

    KorriRpcBinding(String origin, int port, String capability) {
        if (port < 1 || port > 65535 || capability == null
                || !capability.matches("[0-9a-f]{64}")) {
            throw new IllegalArgumentException("Invalid korrid connection");
        }
        this.origin = origin;
        this.port = port;
        this.capability = capability;
    }

    String documentStartScript() {
        return "(() => {"
                + "if (window !== window.top || location.origin !== " + JSONObject.quote(origin) + ") return;"
                + "const notify = window." + READY_LISTENER + ";"
                + "const port = " + port + ";"
                + "const capability = " + JSONObject.quote(capability) + ";"
                + "const binding = Object.freeze({"
                + "korridPort() { notify.postMessage('korridPort'); return port; },"
                + "korridCapability() { notify.postMessage('korridCapability'); return capability; }"
                + "});"
                + "Object.defineProperty(window, 'KorriRpc', { value: binding,"
                + " writable: false, configurable: false, enumerable: false });"
                + "})();";
    }

    void navigationStarted() {
        portRead = false;
        capabilityRead = false;
    }

    boolean consumed(String sourceOrigin, boolean mainFrame, String method) {
        if (!mainFrame || !origin.equals(sourceOrigin)) return false;
        if ("korridPort".equals(method)) portRead = true;
        else if ("korridCapability".equals(method)) capabilityRead = true;
        return portRead && capabilityRead;
    }
}
