package com.limelight;

import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;

/** Orders automatic trust provisioning and process-wide launch reservation. */
final class KorriMoonlightShellFlow {
    interface LaunchAuthorizer {
        String authorize(String specJson) throws Exception;
    }

    interface OnlineHostSource {
        ComputerDetails await(String hostUuid) throws Exception;
    }

    interface ProvisionedAppSource {
        KorriMoonlightProvisioning.Provisioned load(String hostUuid) throws Exception;
    }

    interface LaunchLease {
        void rollback();
    }

    interface ActiveLaunchReservation {
        LaunchLease reserve(String specJson, KorriMoonlightLaunchSpec spec) throws Exception;
    }

    interface GameStarter {
        void start(
                String specJson,
                KorriMoonlightLaunchSpec spec,
                NvApp app,
                ComputerDetails computer) throws Exception;
    }

    static final class Failure extends Exception {
        final String reason;

        Failure(String reason, String message) {
            super(message);
            this.reason = reason;
        }
    }

    private final LaunchAuthorizer launchAuthorizer;
    private final OnlineHostSource onlineHostSource;
    private final ProvisionedAppSource provisionedAppSource;
    private final ActiveLaunchReservation activeLaunchReservation;
    private final GameStarter gameStarter;

    KorriMoonlightShellFlow(
            LaunchAuthorizer launchAuthorizer,
            OnlineHostSource onlineHostSource,
            ProvisionedAppSource provisionedAppSource,
            ActiveLaunchReservation activeLaunchReservation,
            GameStarter gameStarter) {
        this.launchAuthorizer = launchAuthorizer;
        this.onlineHostSource = onlineHostSource;
        this.provisionedAppSource = provisionedAppSource;
        this.activeLaunchReservation = activeLaunchReservation;
        this.gameStarter = gameStarter;
    }

    void startStream(String specJson) throws Failure {
        final String authorization;
        try {
            authorization = launchAuthorizer.authorize(specJson);
        } catch (Exception error) {
            throw failure("StartFailed", "Moonlight launch authorization failed", error);
        }
        if (!"Authorized".equals(authorization)) {
            throw new Failure(
                    "StartFailed",
                    "Moonlight launch instruction rejected: " + authorization);
        }

        final KorriMoonlightLaunchSpec spec;
        try {
            spec = KorriMoonlightLaunchSpec.parse(specJson);
        } catch (KorriMoonlightLaunchSpec.Invalid error) {
            throw new Failure("StartFailed", error.getMessage());
        }

        requireOnlineHost(spec.hostUuid);
        KorriMoonlightProvisioning.Provisioned provisioned =
                requireProvisionedApps(spec.hostUuid);
        final NvApp app;
        try {
            app = KorriMoonlightAppResolver.resolveExpected(spec, provisioned.apps);
        } catch (KorriMoonlightAppResolver.Failure error) {
            throw new Failure(error.reason, error.getMessage());
        }

        final LaunchLease lease;
        try {
            lease = activeLaunchReservation.reserve(specJson, spec);
        } catch (Exception error) {
            throw failure("StartFailed", "Moonlight launch publication failed", error);
        }
        if (lease == null) {
            throw new Failure("StartInProgress", "another game is already active");
        }

        try {
            gameStarter.start(specJson, spec, app, provisioned.computer);
        } catch (Exception error) {
            lease.rollback();
            throw failure("StartFailed", "Moonlight Activity start failed", error);
        }
        // The lease remains active. Game claims it and clears it on termination.
    }

    private ComputerDetails requireOnlineHost(String hostUuid) throws Failure {
        try {
            ComputerDetails computer = onlineHostSource.await(hostUuid);
            if (computer == null) {
                throw new Failure("HostUnreachable", "host is not reachable");
            }
            return computer;
        } catch (Failure failure) {
            throw failure;
        } catch (Exception error) {
            throw failure("HostUnreachable", "host is not reachable", error);
        }
    }

    private KorriMoonlightProvisioning.Provisioned requireProvisionedApps(String hostUuid)
            throws Failure {
        try {
            return provisionedAppSource.load(hostUuid);
        } catch (KorriMoonlightProvisioning.Failure error) {
            throw new Failure(error.reason, error.getMessage());
        } catch (Exception error) {
            throw failure("ProvisioningFailed", "certificate provisioning failed", error);
        }
    }

    private static Failure failure(String reason, String fallback, Exception error) {
        String message = error.getMessage();
        if (message == null || message.contains("BEGIN CERTIFICATE")) message = fallback;
        return new Failure(reason, message);
    }
}
