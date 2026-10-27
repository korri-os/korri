package com.limelight;

import org.junit.Test;

import static org.junit.Assert.assertEquals;

public class KorriNotificationPermissionPromptTest {
    @Test
    public void firstEverStateMayPrompt() {
        assertEquals(
                KorriNotificationPermissionPrompt.Decision.PROMPT,
                KorriNotificationPermissionPrompt.decision(false, false));
    }

    @Test
    public void deniedOnceStateMayPromptAgain() {
        assertEquals(
                KorriNotificationPermissionPrompt.Decision.PROMPT,
                KorriNotificationPermissionPrompt.decision(true, true));
    }

    @Test
    public void exhaustedStateDoesNotPrompt() {
        assertEquals(
                KorriNotificationPermissionPrompt.Decision.UNPROMPTED,
                KorriNotificationPermissionPrompt.decision(true, false));
    }

    @Test
    public void unaskedRationaleStateStillMayPrompt() {
        assertEquals(
                KorriNotificationPermissionPrompt.Decision.PROMPT,
                KorriNotificationPermissionPrompt.decision(false, true));
    }
}
