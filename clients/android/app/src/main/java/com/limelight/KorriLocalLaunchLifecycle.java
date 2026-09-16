package com.limelight;

import java.util.regex.Pattern;

/** Closed, non-secret structured evidence for a successfully published local launch. */
final class KorriLocalLaunchLifecycle {
    static final String TAG = "KorriLocalLifecycle";

    private static final Pattern LAUNCH_ID = Pattern.compile("^[0-9a-f]{32}$");
    private static final Pattern GAME_ID = Pattern.compile("^[A-Za-z0-9._:@/-]+$");
    private static final Pattern PACKAGE = Pattern.compile(
            "^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$");
    private static final Pattern LAUNCHER = Pattern.compile("^[A-Za-z0-9._:@/-]+$");

    private KorriLocalLaunchLifecycle() {}

    static String published(
            String launchId, String gameId, String packageName, String runnerId) {
        require(LAUNCH_ID, launchId, "launchId");
        require(GAME_ID, gameId, "gameId");
        require(PACKAGE, packageName, "package");
        require(LAUNCHER, runnerId, "launcher");
        return "launchId=" + launchId
                + " event=published"
                + " gameId=" + gameId
                + " package=" + packageName
                + " launcher=" + runnerId;
    }

    private static void require(Pattern pattern, String value, String field) {
        if (value == null || !pattern.matcher(value).matches()) {
            throw new IllegalArgumentException("invalid local lifecycle " + field);
        }
    }
}
