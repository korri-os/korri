package com.simonwjackson.korri.korrid;

import android.content.Context;
import android.content.Intent;
import android.os.BatteryManager;
import java.net.InetAddress;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkInfo;
import androidx.test.core.app.ApplicationProvider;
import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowConnectivityManager;
import org.robolectric.shadows.ShadowNetwork;
import org.robolectric.shadows.ShadowNetworkInfo;
import org.robolectric.shadows.ShadowNetworkCapabilities;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28)
public class KorridNetworkTest {
    private static LinkAddress address(String value) throws Exception {
        String[] parts = value.split("/");
        return ReflectionHelpers.callConstructor(LinkAddress.class,
                ClassParameter.from(InetAddress.class, InetAddress.getByName(parts[0])),
                ClassParameter.from(int.class, Integer.parseInt(parts[1])));
    }

    private static void addAddress(LinkProperties link, String value) throws Exception {
        ReflectionHelpers.callInstanceMethod(link, "addLinkAddress", ClassParameter.from(LinkAddress.class, address(value)));
    }

    private static NetworkCapabilities capabilities(int transport) {
        NetworkCapabilities capabilities = ShadowNetworkCapabilities.newInstance();
        Shadows.shadowOf(capabilities).addTransportType(transport);
        return capabilities;
    }

    @Test public void relay_facts_require_observed_power_and_an_unmetered_active_network() throws Exception {
        Context context = ApplicationProvider.getApplicationContext();
        ConnectivityManager manager = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        ShadowConnectivityManager connectivity = Shadows.shadowOf(manager);
        connectivity.setActiveNetworkInfo(ShadowNetworkInfo.newInstance(NetworkInfo.DetailedState.CONNECTED, ConnectivityManager.TYPE_WIFI, 0, true, true));
        Network active = manager.getActiveNetwork();
        assertNotNull(active);
        NetworkCapabilities capabilities = capabilities(NetworkCapabilities.TRANSPORT_WIFI);
        Shadows.shadowOf(capabilities).addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        connectivity.setNetworkCapabilities(active, capabilities);
        LinkProperties link = new LinkProperties();
        link.setInterfaceName("wlan0");
        connectivity.setLinkProperties(active, link);
        context.sendStickyBroadcast(new Intent(Intent.ACTION_BATTERY_CHANGED).putExtra(BatteryManager.EXTRA_PLUGGED, 0));
        JSONObject battery = new JSONObject(KorridNetwork.snapshot(context));
        assertTrue(battery.getBoolean("unmetered"));
        assertFalse(battery.getBoolean("external_power"));
        context.sendStickyBroadcast(new Intent(Intent.ACTION_BATTERY_CHANGED).putExtra(BatteryManager.EXTRA_PLUGGED, BatteryManager.BATTERY_PLUGGED_USB));
        JSONObject powered = new JSONObject(KorridNetwork.snapshot(context));
        assertTrue(powered.getBoolean("unmetered"));
        assertTrue(powered.getBoolean("external_power"));
        Shadows.shadowOf(capabilities).removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        JSONObject metered = new JSONObject(KorridNetwork.snapshot(context));
        assertFalse(metered.getBoolean("unmetered"));
        assertTrue(metered.getBoolean("external_power"));
        context.sendStickyBroadcast(new Intent(Intent.ACTION_BATTERY_CHANGED).putExtra(BatteryManager.EXTRA_PLUGGED, -1));
        assertFalse(new JSONObject(KorridNetwork.snapshot(context)).getBoolean("external_power"));
    }

    @Test public void no_network_evidence_denies_relay_and_reports_no_interfaces() throws Exception {
        Context context = ApplicationProvider.getApplicationContext();
        ConnectivityManager manager = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        ShadowConnectivityManager connectivity = Shadows.shadowOf(manager);
        connectivity.clearAllNetworks();
        connectivity.setActiveNetworkInfo(null);
        JSONObject snapshot = new JSONObject(KorridNetwork.snapshot(context));
        assertEquals(0, snapshot.getJSONArray("interfaces").length());
        assertFalse(snapshot.getBoolean("unmetered"));
    }

    @Test public void physical_addresses_reach_rust_but_vpn_and_loopback_do_not() throws Exception {
        Context context = ApplicationProvider.getApplicationContext();
        ConnectivityManager manager = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        ShadowConnectivityManager connectivity = Shadows.shadowOf(manager);
        connectivity.clearAllNetworks();
        connectivity.setActiveNetworkInfo(null);
        Network wifi = ShadowNetwork.newInstance(71);
        Network vpn = ShadowNetwork.newInstance(72);
        connectivity.addNetwork(wifi, ShadowNetworkInfo.newInstance(NetworkInfo.DetailedState.CONNECTED, ConnectivityManager.TYPE_WIFI, 0, true, true));
        connectivity.addNetwork(vpn, ShadowNetworkInfo.newInstance(NetworkInfo.DetailedState.CONNECTED, ConnectivityManager.TYPE_VPN, 0, true, true));
        connectivity.setNetworkCapabilities(wifi, capabilities(NetworkCapabilities.TRANSPORT_WIFI));
        connectivity.setNetworkCapabilities(vpn, capabilities(NetworkCapabilities.TRANSPORT_VPN));
        LinkProperties physical = new LinkProperties();
        physical.setInterfaceName("wlan0");
        addAddress(physical, "192.0.2.44/24");
        addAddress(physical, "2001:db8::44/64");
        addAddress(physical, "127.0.0.1/8");
        connectivity.setLinkProperties(wifi, physical);
        LinkProperties tunneled = new LinkProperties();
        tunneled.setInterfaceName("tun0");
        addAddress(tunneled, "100.64.0.3/24");
        connectivity.setLinkProperties(vpn, tunneled);
        JSONArray addresses = new JSONObject(KorridNetwork.snapshot(context)).getJSONArray("interfaces");
        assertEquals(2, addresses.length());
        for (int i = 0; i < addresses.length(); i++) {
            assertEquals("wlan0", addresses.getJSONObject(i).getString("interface"));
            assertFalse(addresses.getJSONObject(i).getBoolean("has_default_route"));
        }
    }
}
