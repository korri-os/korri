package com.limelight;

import android.net.Uri;

import java.util.Locale;

/**
 * Main-shell WebView policy for the privileged Korri portal.
 *
 * The RPC binding exposes localhost authority. Release builds therefore
 * trust only the bundled appassets portal. Debug builds may
 * opt into one configured http(s) portal origin for live development, while
 * game-art assets remain pinned to the fixed appassets route.
 */
final class KorriTrustedPortalWebViewPolicy {
    static final String TRUSTED_PORTAL_URL =
            "https://appassets.androidplatform.net/assets/portal/index.html";

    enum NavigationAction {
        ALLOW_IN_WEBVIEW,
        OPEN_EXTERNALLY,
        BLOCK
    }

    private final Uri portalUrl;
    private final Origin portalOrigin;
    private final boolean bundledPortal;

    KorriTrustedPortalWebViewPolicy() {
        this(Uri.parse(TRUSTED_PORTAL_URL), true);
    }

    KorriTrustedPortalWebViewPolicy(Uri portalUrl, boolean bundledPortal) {
        Origin origin = Origin.from(portalUrl);
        if (origin == null) {
            throw new IllegalArgumentException("portal URL has no origin");
        }
        if (bundledPortal) {
            if (!origin.equals(Origin.from(Uri.parse(TRUSTED_PORTAL_URL)))
                    || !isTrustedAssetPath(portalUrl)) {
                throw new IllegalArgumentException("bundled portal URL must be a trusted asset URL");
            }
        } else if (!isHttpOrHttps(origin)) {
            throw new IllegalArgumentException("debug portal URL must be http(s)");
        }
        this.portalUrl = portalUrl;
        this.portalOrigin = origin;
        this.bundledPortal = bundledPortal;
    }

    static KorriTrustedPortalWebViewPolicy forRuntime(boolean debugBuild, String devPortalUrl) {
        if (debugBuild && devPortalUrl != null && !devPortalUrl.isEmpty()) {
            return new KorriTrustedPortalWebViewPolicy(Uri.parse(devPortalUrl), false);
        }
        return new KorriTrustedPortalWebViewPolicy();
    }

    String portalUrl() {
        return portalUrl.toString();
    }

    String portalOrigin() {
        return portalOrigin.toString();
    }

    boolean isBundledPortalAsset(Uri uri) {
        Origin origin = Origin.from(uri);
        return origin != null
                && origin.equals(Origin.from(Uri.parse(TRUSTED_PORTAL_URL)))
                && isTrustedAssetPath(uri);
    }

    boolean isTrustedPortalResource(Uri uri) {
        Origin origin = Origin.from(uri);
        if (origin == null || !portalOrigin.equals(origin)) {
            return false;
        }
        if (bundledPortal) {
            return isTrustedAssetPath(uri);
        }
        return true;
    }

    boolean isTrustedLocalGameAsset(Uri uri) {
        Origin origin = Origin.from(uri);
        if (origin == null
                || !origin.equals(Origin.from(Uri.parse(TRUSTED_PORTAL_URL)))
                || uri == null
                || uri.isOpaque()) {
            return false;
        }
        if (uri.getQuery() != null || uri.getFragment() != null) {
            return false;
        }
        String encodedPath = uri.getEncodedPath();
        if (encodedPath == null || encodedPath.contains("%")) {
            return false;
        }
        if (uri.getPathSegments().size() != 2
                || !"game-assets".equals(uri.getPathSegments().get(0))) {
            return false;
        }
        String assetId = uri.getPathSegments().get(1);
        return KorriGameAssetPathHandler.isWellFormedAssetId(assetId)
                && encodedPath.equals(KorriGameAssetPathHandler.ROUTE_PREFIX + assetId);
    }

    NavigationAction navigationAction(Uri uri, boolean mainFrame) {
        if (isTrustedPortalResource(uri)) {
            return NavigationAction.ALLOW_IN_WEBVIEW;
        }
        if (mainFrame && isExternalBrowserUrl(uri)) {
            return NavigationAction.OPEN_EXTERNALLY;
        }
        return NavigationAction.BLOCK;
    }

    private static boolean isTrustedAssetPath(Uri uri) {
        if (uri == null || uri.isOpaque()) {
            return false;
        }
        return uri.getPathSegments().size() >= 2
                && "assets".equals(uri.getPathSegments().get(0));
    }

    private static boolean isExternalBrowserUrl(Uri uri) {
        Origin origin = Origin.from(uri);
        if (origin == null || origin.isLoopback()) {
            return false;
        }
        return isHttpOrHttps(origin);
    }

    private static boolean isHttpOrHttps(Origin origin) {
        return "https".equals(origin.scheme) || "http".equals(origin.scheme);
    }

    private static final class Origin {
        private final String scheme;
        private final String host;
        private final int port;

        private Origin(String scheme, String host, int port) {
            this.scheme = scheme;
            this.host = host;
            this.port = port;
        }

        static Origin from(Uri uri) {
            if (uri == null || uri.isOpaque()) {
                return null;
            }
            String scheme = uri.getScheme();
            String host = uri.getHost();
            if (scheme == null || host == null || scheme.isEmpty() || host.isEmpty()) {
                return null;
            }
            String normalizedScheme = scheme.toLowerCase(Locale.ROOT);
            int port = uri.getPort();
            if (port == -1) {
                port = defaultPort(normalizedScheme);
            }
            if (port == -1) {
                return null;
            }
            return new Origin(normalizedScheme, host.toLowerCase(Locale.ROOT), port);
        }

        private static int defaultPort(String scheme) {
            if ("https".equals(scheme)) {
                return 443;
            }
            if ("http".equals(scheme)) {
                return 80;
            }
            return -1;
        }

        boolean isLoopback() {
            return "localhost".equals(host)
                    || "127.0.0.1".equals(host)
                    || "::1".equals(host)
                    || "[::1]".equals(host);
        }

        @Override
        public boolean equals(Object other) {
            if (!(other instanceof Origin)) {
                return false;
            }
            Origin origin = (Origin) other;
            return scheme.equals(origin.scheme)
                    && host.equals(origin.host)
                    && port == origin.port;
        }

        @Override
        public int hashCode() {
            int result = scheme.hashCode();
            result = 31 * result + host.hashCode();
            result = 31 * result + port;
            return result;
        }

        @Override
        public String toString() {
            boolean defaultPort = ("https".equals(scheme) && port == 443)
                    || ("http".equals(scheme) && port == 80);
            return defaultPort ? scheme + "://" + host : scheme + "://" + host + ":" + port;
        }
    }
}
