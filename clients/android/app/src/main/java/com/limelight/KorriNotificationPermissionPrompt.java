package com.limelight;

final class KorriNotificationPermissionPrompt {
    enum Decision {
        PROMPT,
        UNPROMPTED
    }

    private KorriNotificationPermissionPrompt() {}

    static Decision decision(boolean asked, boolean rationale) {
        if (!asked || rationale) {
            return Decision.PROMPT;
        }
        return Decision.UNPROMPTED;
    }
}
