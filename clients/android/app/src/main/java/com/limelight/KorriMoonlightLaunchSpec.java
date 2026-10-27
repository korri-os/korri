package com.limelight;

import com.limelight.nvstream.http.NvApp;

import org.json.JSONObject;

import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Closed Java mirror of contracts/generated/korrid.ts MoonlightLaunchSpec. */
final class KorriMoonlightLaunchSpec {
    private static final String TRANSPORT = "@korri:moonlight/moonlight";
    private static final String IMPLEMENTATION = "artemis";
    private static final Set<String> FIELDS = new HashSet<>(Arrays.asList(
            "launchId",
            "transportId",
            "context",
            "implementation",
            "sunshineApp",
            "hostUuid",
            "appId",
            "integrity"));

    final String launchId;
    final String transportId;
    final String implementation;
    final String sunshineApp;
    final String hostUuid;
    final int appId;

    private KorriMoonlightLaunchSpec(
            String launchId,
            String transportId,
            String implementation,
            String sunshineApp,
            String hostUuid,
            int appId) {
        this.launchId = launchId;
        this.transportId = transportId;
        this.implementation = implementation;
        this.sunshineApp = sunshineApp;
        this.hostUuid = hostUuid;
        this.appId = appId;
    }

    static KorriMoonlightLaunchSpec parse(String json) throws Invalid {
        try {
            JSONObject value = new JSONObject(json);
            Set<String> fields = new HashSet<>();
            value.keys().forEachRemaining(fields::add);
            if (!FIELDS.equals(fields)) {
                throw new Invalid("Moonlight launch fields do not match the closed treaty");
            }
            String launchId = value.getString("launchId");
            String transportId = value.getString("transportId");
            String implementation = value.getString("implementation");
            String sunshineApp = value.getString("sunshineApp");
            String hostUuid = value.getString("hostUuid");
            int appId = value.getInt("appId");
            String integrity = value.getString("integrity");
            if (launchId.length() != 32
                    || !TRANSPORT.equals(transportId)
                    || !IMPLEMENTATION.equals(implementation)
                    || sunshineApp.isEmpty()
                    || hostUuid.isEmpty()
                    || appId <= 0
                    || integrity.isEmpty()) {
                throw new Invalid("Moonlight launch values are invalid");
            }
            return new KorriMoonlightLaunchSpec(
                    launchId, transportId, implementation, sunshineApp, hostUuid, appId);
        } catch (Invalid error) {
            throw error;
        } catch (Exception error) {
            throw new Invalid("Moonlight launch instruction is not valid JSON");
        }
    }

    NvApp selectExpectedApp(List<NvApp> apps) {
        for (NvApp app : apps) {
            if (app.getAppId() == appId && sunshineApp.equals(app.getAppName())) {
                return app;
            }
        }
        return null;
    }

    static final class Invalid extends Exception {
        final String reason = "InvalidSpec";

        Invalid(String message) {
            super(message);
        }
    }
}
