package com.limelight;

import android.content.Context;

import com.limelight.binding.PlatformBinding;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.utils.ServerHelper;

import java.util.List;

/** Refreshes the trusted host's Artemis app list and resolves only the app
 * identity carried by korrid's signed Moonlight launch instruction. */
final class KorriMoonlightAppResolver {
    interface AppListQuery {
        List<NvApp> refresh() throws Exception;
    }

    private final AppListQuery query;

    KorriMoonlightAppResolver(AppListQuery query) {
        this.query = query;
    }

    static KorriMoonlightAppResolver artemis(
            Context context,
            ComputerManagerService.ComputerManagerBinder binder,
            ComputerDetails computer) {
        return new KorriMoonlightAppResolver(() -> {
            NvHTTP http = new NvHTTP(
                    ServerHelper.getCurrentAddressFromComputer(computer),
                    computer.httpsPort,
                    binder.getUniqueId(),
                    computer.serverCert,
                    PlatformBinding.getCryptoProvider(context));
            return http.getAppList();
        });
    }

    NvApp refreshExpected(KorriMoonlightLaunchSpec spec) throws Failure {
        final List<NvApp> current;
        try {
            current = query.refresh();
        } catch (Exception error) {
            throw new Failure(
                    "StartFailed",
                    "failed to refresh current host app list: "
                            + (error.getMessage() != null
                            ? error.getMessage()
                            : error.getClass().getSimpleName()));
        }
        return resolveExpected(spec, current);
    }

    static NvApp resolveExpected(KorriMoonlightLaunchSpec spec, List<NvApp> current)
            throws Failure {
        NvApp expected = spec.selectExpectedApp(current);
        if (expected == null) {
            throw new Failure(
                    "AppNotFound",
                    "current app " + spec.appId
                            + " does not match plugin-owned app " + spec.sunshineApp);
        }
        return expected;
    }

    static final class Failure extends Exception {
        final String reason;

        Failure(String reason, String message) {
            super(message);
            this.reason = reason;
        }
    }
}
