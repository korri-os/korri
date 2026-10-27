package org.korri.signer.test;

import android.content.Context;
import android.content.SharedPreferences;

final class SignerBehavior {
    enum Kind {
        APPROVE,
        DENY,
        DELAY,
        MALFORMED,
        WRONG_EVENT
    }

    private static final String PREFS = "signer-test";
    private static final String BEHAVIOR = "behavior";
    private static final String ACCOUNT_SELECTED = "account-selected";

    static Kind read(Context context) {
        String value = preferences(context).getString(BEHAVIOR, Kind.APPROVE.name());
        try {
            return Kind.valueOf(value);
        } catch (IllegalArgumentException error) {
            return Kind.APPROVE;
        }
    }

    static void write(Context context, Kind behavior) {
        preferences(context).edit().putString(BEHAVIOR, behavior.name()).apply();
    }

    static boolean accountSelected(Context context) {
        return preferences(context).getBoolean(ACCOUNT_SELECTED, false);
    }

    static void selectAccount(Context context) {
        preferences(context).edit().putBoolean(ACCOUNT_SELECTED, true).apply();
    }

    private static SharedPreferences preferences(Context context) {
        return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    private SignerBehavior() {}
}
