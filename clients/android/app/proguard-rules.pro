# Don't obfuscate code
-dontobfuscate

# Our code
-keep class com.limelight.binding.input.evdev.* {*;}

# KeyMapper - keep all VK_* fields for reflection
-keep class com.limelight.utils.KeyMapper {*;}

# KeyConfigHelper - keep classes and fields for Gson
-keep class com.limelight.utils.KeyConfigHelper {*;}
-keep class com.limelight.utils.KeyConfigHelper$ShortcutFile {*;}
-keep class com.limelight.utils.KeyConfigHelper$Shortcut {*;}

# Moonlight common
-keep class com.limelight.nvstream.jni.* {*;}

# Okio
-keep class sun.misc.Unsafe {*;}
-dontwarn java.nio.file.*
-dontwarn org.codehaus.mojo.animal_sniffer.IgnoreJRERequirement
-dontwarn okio.**

# BouncyCastle
-keep class org.bouncycastle.jcajce.provider.asymmetric.* {*;}
-keep class org.bouncycastle.jcajce.provider.asymmetric.util.* {*;}
-keep class org.bouncycastle.jcajce.provider.asymmetric.rsa.* {*;}
-keep class org.bouncycastle.jcajce.provider.digest.** {*;}
-keep class org.bouncycastle.jcajce.provider.symmetric.** {*;}
-keep class org.bouncycastle.jcajce.spec.* {*;}
-keep class org.bouncycastle.jce.** {*;}
-dontwarn javax.naming.**

# THROWAWAY Rust/JNI spike: native symbols include their Java method names.
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}

# jMDNS
-dontwarn javax.jmdns.impl.DNSCache
-dontwarn org.slf4j.**

# Rust polls Android network evidence through the captured application class.
-keepclassmembers class com.simonwjackson.korri.korrid.KorridServer {
    public static java.lang.String networkSnapshot(android.content.Context);
    public static java.lang.String viewerStart(java.lang.String, java.lang.String, int, int);
    public static java.lang.String viewerDisplay();
    public static java.lang.String viewerStatus(java.lang.String);
    public static java.lang.String viewerBackground(java.lang.String);
    public static java.lang.String viewerResume(java.lang.String);
    public static java.lang.String viewerStop(java.lang.String);
}

# Called through JNI by iroh's TLS certificate verifier.
-keep,includedescriptorclasses class org.rustls.platformverifier.** { *; }

# Slint's android_main reads this private in-process bootstrap through JNI.
-keepclassmembers class com.limelight.KorriShellActivity {
    public int shellPort();
    public java.lang.String shellOrigin();
    public java.lang.String shellCapability();
    public java.lang.String shellPackagePath();
    public int[] shellTakeRemovedInputDevices();
    public void shellRendered(java.lang.String, java.lang.String, int, int);
    public java.lang.String shellSurfaceId();
    public java.lang.String shellSurfacePage();
    public int shellLibraryCount();
    public int shellSelectedIndex();
    public boolean isShellReady();
    public android.view.Surface shellTakeOverlaySurface();
    public float shellOverlayDensity();
    public boolean shellOverlayResized();
    public boolean shellOverlayClosing();
    public void shellOverlayReleased(boolean);
    public int[] shellTakeOverlayKeys();
    public int[] shellTakeOverlayAxes();
}

# ADR 0054: Rust calls these static methods through JNI by name.
-keep class com.simonwjackson.korri.korrid.KorriAppLauncher {
    public static java.lang.String *(...);
}
# The save bridge provider is named in Korri-built game APKs' manifests and
# its client is called from KorriAppLauncher.
-keep class dev.korri.savebridge.** { *; }
