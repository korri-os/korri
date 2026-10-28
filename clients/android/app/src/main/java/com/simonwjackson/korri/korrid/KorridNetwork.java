package com.simonwjackson.korri.korrid;

import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.RouteInfo;
import android.os.BatteryManager;
import android.os.Build;
import java.net.InetAddress;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Live Android observations for the existing Rust interface and relay-facts consumers. */
final class KorridNetwork {
    private KorridNetwork() {}

    static String snapshot(Context context) {
        ConnectivityManager connectivity = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (connectivity == null) throw new IllegalStateException("Network service is unavailable");
        Network active = Build.VERSION.SDK_INT >= 23 ? connectivity.getActiveNetwork() : null;
        JSONArray interfaces = new JSONArray();
        boolean unmetered = false;
        try {
            for (Network network : connectivity.getAllNetworks()) {
                NetworkCapabilities capabilities = connectivity.getNetworkCapabilities(network);
                LinkProperties link = connectivity.getLinkProperties(network);
                // QUIC uses physical links. VPNs remain available to ordinary
                // peer HTTP through Android routing, never as a required path.
                if (capabilities == null || capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
                        || link == null || link.getInterfaceName() == null) continue;
                boolean defaultRoute = network.equals(active);
                if (active == null) {
                    for (RouteInfo route : link.getRoutes()) defaultRoute |= route.isDefaultRoute();
                }
                if (network.equals(active)) {
                    unmetered = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
                }
                for (LinkAddress address : link.getLinkAddresses()) {
                    InetAddress ip = address.getAddress();
                    if (ip.isLoopbackAddress() || ip.isAnyLocalAddress() || ip.isMulticastAddress() || ip.isLinkLocalAddress()) continue;
                    interfaces.put(new JSONObject()
                            .put("interface", link.getInterfaceName())
                            .put("address", ip.getHostAddress())
                            .put("has_default_route", defaultRoute));
                }
            }
            Intent battery = context.registerReceiver(null, new IntentFilter(Intent.ACTION_BATTERY_CHANGED));
            boolean externalPower = battery != null && battery.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) > 0;
            return new JSONObject().put("interfaces", interfaces)
                    .put("external_power", externalPower).put("unmetered", unmetered).toString();
        } catch (JSONException error) {
            throw new IllegalStateException("Could not encode Android network observations", error);
        }
    }
}
