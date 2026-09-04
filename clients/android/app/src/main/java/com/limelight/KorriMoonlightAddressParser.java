package com.limelight;

import android.net.Uri;

import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvHTTP;

/** Parses korrid's explicit Moonlight address with Artemis manual-address semantics. */
final class KorriMoonlightAddressParser {
    private KorriMoonlightAddressParser() {
    }

    static ComputerDetails.AddressTuple parse(String explicitAddress) {
        if (explicitAddress == null || explicitAddress.trim().isEmpty()) return null;
        String input = explicitAddress.trim();
        try {
            Uri uri = parseRawUserInputToUri(input);
            if (uri == null) return null;
            int port = uri.getPort();
            if (port == -1) port = NvHTTP.DEFAULT_HTTP_PORT;
            return new ComputerDetails.AddressTuple(uri.getHost(), port);
        } catch (RuntimeException ignored) {
            return null;
        }
    }

    private static Uri parseRawUserInputToUri(String rawUserInput) {
        // Keep this in step with AddComputerManually: Artemis accepts host, host:port,
        // bracketed IPv6, and unbracketed IPv6 through the second parse.
        Uri uri = Uri.parse("art://" + rawUserInput);
        if (uri.getHost() != null && !uri.getHost().isEmpty()) return uri;

        uri = Uri.parse("art://[" + rawUserInput + "]");
        if (uri.getHost() != null && !uri.getHost().isEmpty()) return uri;

        return null;
    }
}
