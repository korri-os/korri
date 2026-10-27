package com.limelight.utils;

import android.app.Activity;
import android.content.Intent;
import android.os.Build;
import android.widget.Toast;

import androidx.annotation.RequiresApi;

import com.limelight.AppView;
import com.limelight.Game;
import com.limelight.R;
import com.limelight.binding.PlatformBinding;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostHttpResponseException;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.nvstream.jni.MoonBridge;

import org.xmlpull.v1.XmlPullParserException;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.net.UnknownHostException;
import java.security.cert.CertificateEncodingException;

public class ServerHelper {
    public static final String CONNECTION_TEST_SERVER = "android.conntest.moonlight-stream.org";

    public static ComputerDetails.AddressTuple getCurrentAddressFromComputer(ComputerDetails computer) throws IOException {
        if (computer.activeAddress == null) {
            throw new IOException("No active address for "+computer.name);
        }
        return computer.activeAddress;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder) {
        Intent gameIntent = new Intent(parent, Game.class);
        gameIntent.putExtra(Game.EXTRA_HOST, computer.activeAddress.address);
        gameIntent.putExtra(Game.EXTRA_PORT, computer.activeAddress.port);
        gameIntent.putExtra(Game.EXTRA_HTTPS_PORT, computer.httpsPort);
        gameIntent.putExtra(Game.EXTRA_APP_NAME, app.getAppName());
        gameIntent.putExtra(Game.EXTRA_APP_UUID, app.getAppUUID());
        gameIntent.putExtra(Game.EXTRA_APP_ID, app.getAppId());
        gameIntent.putExtra(Game.EXTRA_APP_HDR, app.isHdrSupported());
        gameIntent.putExtra(Game.EXTRA_UNIQUEID, managerBinder.getUniqueId());
        gameIntent.putExtra(Game.EXTRA_PC_UUID, computer.uuid);
        gameIntent.putExtra(Game.EXTRA_PC_NAME, computer.name);

        try {
            if (computer.serverCert != null) {
                gameIntent.putExtra(Game.EXTRA_SERVER_CERT, computer.serverCert.getEncoded());
            }
        } catch (CertificateEncodingException e) {
            e.printStackTrace();
        }

        return gameIntent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder);

        if (overrideWidth > 0 && overrideHeight > 0) {
            intent.putExtra(Game.EXTRA_WIDTH, overrideWidth);
            intent.putExtra(Game.EXTRA_HEIGHT, overrideHeight);
        }

        if (overrideFps > 0) {
            intent.putExtra(Game.EXTRA_FPS, overrideFps);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps, int overrideBitrate) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder, overrideWidth, overrideHeight, overrideFps);

        if (overrideBitrate > 0) {
            intent.putExtra(Game.EXTRA_BITRATE, overrideBitrate);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps, overrideBitrate);

        if (overrideFramePacing != null) {
            intent.putExtra(Game.EXTRA_FRAME_PACING, overrideFramePacing);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing);

        if (overrideUltraLowLatency != null) {
            intent.putExtra(Game.EXTRA_ULTRA_LOW_LATENCY, overrideUltraLowLatency);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing, overrideUltraLowLatency);

        if (overrideVideoScaleMode != null) {
            intent.putExtra(Game.EXTRA_VIDEO_SCALE_MODE, overrideVideoScaleMode);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode);

        if (overrideCodec != null) {
            intent.putExtra(Game.EXTRA_CODEC, overrideCodec);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode, overrideCodec);

        if (overrideDisplayTopCenter != null) {
            intent.putExtra(Game.EXTRA_DISPLAY_TOP_CENTER, overrideDisplayTopCenter);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter);

        if (overrideReduceRefreshRate != null) {
            intent.putExtra(Game.EXTRA_REDUCE_REFRESH_RATE, overrideReduceRefreshRate);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter, overrideReduceRefreshRate);

        if (overrideLowLatencyFrameBalance != null) {
            intent.putExtra(Game.EXTRA_LOW_LATENCY_FRAME_BALANCE, overrideLowLatencyFrameBalance);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance);

        if (overrideTightVsync != null) {
            intent.putExtra(Game.EXTRA_TIGHT_VSYNC, overrideTightVsync);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync);

        if (overridePip != null) {
            intent.putExtra(Game.EXTRA_PIP, overridePip);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip);

        if (overrideAutoOrientation != null) {
            intent.putExtra(Game.EXTRA_AUTO_ORIENTATION, overrideAutoOrientation);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation, String overrideFlipFaceButtons) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip, overrideAutoOrientation);

        if (overrideFlipFaceButtons != null) {
            intent.putExtra(Game.EXTRA_FLIP_FACE_BUTTONS, overrideFlipFaceButtons);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation, String overrideFlipFaceButtons,
                                           String overrideHdr) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip,
                                           overrideAutoOrientation, overrideFlipFaceButtons);

        if (overrideHdr != null) {
            intent.putExtra(Game.EXTRA_HDR, overrideHdr);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation, String overrideFlipFaceButtons,
                                           String overrideHdr, String overrideMouseEmulation) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip,
                                           overrideAutoOrientation, overrideFlipFaceButtons, overrideHdr);

        if (overrideMouseEmulation != null) {
            intent.putExtra(Game.EXTRA_MOUSE_EMULATION, overrideMouseEmulation);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation, String overrideFlipFaceButtons,
                                           String overrideHdr, String overrideMouseEmulation,
                                           String overrideTouchscreenMode) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip,
                                           overrideAutoOrientation, overrideFlipFaceButtons,
                                           overrideHdr, overrideMouseEmulation);

        if (overrideTouchscreenMode != null) {
            intent.putExtra(Game.EXTRA_TOUCHSCREEN_MODE, overrideTouchscreenMode);
        }

        return intent;
    }

    public static Intent createStartIntent(Activity parent, NvApp app, ComputerDetails computer,
                                           ComputerManagerService.ComputerManagerBinder managerBinder,
                                           int overrideWidth, int overrideHeight, int overrideFps,
                                           int overrideBitrate, String overrideFramePacing,
                                           String overrideUltraLowLatency, String overrideVideoScaleMode,
                                           String overrideCodec, String overrideDisplayTopCenter,
                                           String overrideReduceRefreshRate, String overrideLowLatencyFrameBalance,
                                           String overrideTightVsync, String overridePip,
                                           String overrideAutoOrientation, String overrideFlipFaceButtons,
                                           String overrideHdr, String overrideMouseEmulation,
                                           String overrideTouchscreenMode, String overrideAbsoluteMouseMode) {
        Intent intent = createStartIntent(parent, app, computer, managerBinder,
                                           overrideWidth, overrideHeight, overrideFps,
                                           overrideBitrate, overrideFramePacing,
                                           overrideUltraLowLatency, overrideVideoScaleMode,
                                           overrideCodec, overrideDisplayTopCenter,
                                           overrideReduceRefreshRate, overrideLowLatencyFrameBalance,
                                           overrideTightVsync, overridePip,
                                           overrideAutoOrientation, overrideFlipFaceButtons,
                                           overrideHdr, overrideMouseEmulation, overrideTouchscreenMode);

        if (overrideAbsoluteMouseMode != null) {
            intent.putExtra(Game.EXTRA_ABSOLUTE_MOUSE_MODE, overrideAbsoluteMouseMode);
        }

        return intent;
    }

    @RequiresApi(api = Build.VERSION_CODES.O)
    public static void doStart(
            Activity parent,
            NvApp app,
            ComputerDetails computer,
            ComputerManagerService.ComputerManagerBinder managerBinder
    ) {
        if (computer.state == ComputerDetails.State.OFFLINE || computer.activeAddress == null) {
            Toast.makeText(parent, parent.getString(R.string.error_pc_offline), Toast.LENGTH_SHORT).show();
            return;
        }

        Intent intent = createStartIntent(parent, app, computer, managerBinder);
        parent.startActivity(intent);
    }

    public static void doNetworkTest(final Activity parent) {
        new Thread(new Runnable() {
            @Override
            public void run() {
                SpinnerDialog spinnerDialog = SpinnerDialog.displayDialog(parent,
                        parent.getResources().getString(R.string.nettest_title_waiting),
                        parent.getResources().getString(R.string.nettest_text_waiting),
                        false);

                int ret = MoonBridge.testClientConnectivity(CONNECTION_TEST_SERVER, 443, MoonBridge.ML_PORT_FLAG_ALL);
                spinnerDialog.dismiss();

                String dialogSummary;
                if (ret == MoonBridge.ML_TEST_RESULT_INCONCLUSIVE) {
                    dialogSummary = parent.getResources().getString(R.string.nettest_text_inconclusive);
                }
                else if (ret == 0) {
                    dialogSummary = parent.getResources().getString(R.string.nettest_text_success);
                }
                else {
                    dialogSummary = parent.getResources().getString(R.string.nettest_text_failure);
                    dialogSummary += MoonBridge.stringifyPortFlags(ret, "\n");
                }

                Dialog.displayDialog(parent,
                        parent.getResources().getString(R.string.nettest_title_done),
                        dialogSummary,
                        false);
            }
        }).start();
    }

    public static void doQuit(final Activity parent,
                              final NvHTTP httpConn,
                              final String appName,
                              final Runnable onComplete,
                              final Runnable onFail
    ) {
        parent.runOnUiThread(() -> Toast.makeText(parent, parent.getResources().getString(R.string.applist_quit_app) + " " + appName + "...", Toast.LENGTH_SHORT).show());
        new Thread(new Runnable() {
            @Override
            public void run() {
                String message;
                boolean failed = false;
                try {
                    if (httpConn.quitApp()) {
                        message = parent.getResources().getString(R.string.applist_quit_success) + " " + appName;
                    } else {
                        message = parent.getResources().getString(R.string.applist_quit_fail) + " " + appName;
                    }
                } catch (HostHttpResponseException e) {
                    failed = true;
                    if (e.getErrorCode() == 599) {
                        message = "This session wasn't started by this device," +
                                " so it cannot be quit. End streaming on the original " +
                                "device or the PC itself. (Error code: "+e.getErrorCode()+")";
                    }
                    else {
                        message = e.getMessage();
                    }
                } catch (UnknownHostException e) {
                    failed = true;
                    message = parent.getResources().getString(R.string.error_unknown_host);
                } catch (FileNotFoundException e) {
                    failed = true;
                    message = parent.getResources().getString(R.string.error_404);
                } catch (IOException | XmlPullParserException e) {
                    failed = true;
                    message = e.getMessage();
                    e.printStackTrace();
                } finally {
                    if (failed) {
                        if (onFail != null) {
                            onFail.run();
                        }
                    } else {
                        if (onComplete != null) {
                            onComplete.run();
                        }
                    }
                }

                final String toastMessage = message;
                parent.runOnUiThread(() -> Toast.makeText(parent, toastMessage, Toast.LENGTH_LONG).show());
            }
        }).start();

    }

    public static void doQuit(final Activity parent,
                              final ComputerDetails computer,
                              final NvApp app,
                              final ComputerManagerService.ComputerManagerBinder managerBinder,
                              final Runnable onComplete
    ) {
        try {
            NvHTTP httpConn = new NvHTTP(
                    ServerHelper.getCurrentAddressFromComputer(computer),
                    computer.httpsPort,
                    managerBinder.getUniqueId(),
                    computer.serverCert,
                    PlatformBinding.getCryptoProvider(parent)
            );
            doQuit(
                    parent,
                    httpConn,
                    app.getAppName(),
                    onComplete,
                    null
            );
        } catch (Exception e) {
            e.printStackTrace();

            final String toastMessage = e.getMessage();
            parent.runOnUiThread(() -> Toast.makeText(parent, toastMessage, Toast.LENGTH_LONG).show());
        }
    }
}
