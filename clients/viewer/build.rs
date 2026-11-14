// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf};
fn main() {
    let include =
        env::var("MOONLIGHT_COMMON_INCLUDE").expect("use the viewer Nix development shell");
    let lib = env::var("MOONLIGHT_COMMON_LIB").expect("missing network library path");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_INCLUDE");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_LIB");
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    // Android carries none of the Linux picture and I/O C code. android.nix
    // builds the same core as static archives for the session part alone.
    let android = os == "android";
    // The handheld path (Moonlight's v4l2request decoder) is aarch64 Linux
    // only. It needs FFmpeg with the V4L2 Request hwaccel; default.nix
    // supplies it.
    let request = os == "linux" && env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64";
    println!("cargo::rustc-check-cfg=cfg(v4l2request)");
    if request {
        println!("cargo::rustc-cfg=v4l2request");
    }
    let headers = if android {
        vec![format!("{include}/Limelight.h")]
    } else {
        vec!["picture/bridge.h".into(), "io/bridge.h".into()]
    };
    let bindings = headers
        .iter()
        .fold(bindgen::Builder::default(), |builder, header| {
            builder.header(header)
        })
        .clang_arg(format!("-I{include}"))
        .allowlist_function("Li(Initialize(ServerInformation|StreamConfiguration|ConnectionCallbacks)|StartConnection|StopConnection|InterruptConnection|GetLaunchUrlQueryParameters|GetMicroseconds)")
        .allowlist_function("picture_.*|viewer_.*")
        .allowlist_type("(STREAM_CONFIGURATION|SERVER_INFORMATION|CONNECTION_LISTENER_CALLBACKS|DECODER_RENDERER_CALLBACKS|AUDIO_RENDERER_CALLBACKS|DECODE_UNIT|LENTRY)")
        // Android's decoder (src/android/picture.rs) reads decode units.
        .allowlist_var("(VIDEO_FORMAT_.*|STREAM_CFG_.*|ENCFLG_.*|BUFFER_TYPE_.*|FRAME_TYPE_.*|DR_.*)")
        .derive_default(true)
        .generate().expect("generate binding from the pinned core's actual header");
    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("network.rs"))
        .unwrap();
    if android {
        // The core's log callback is C-variadic; it writes to logcat.
        cc::Build::new()
            .file("src/android/log.c")
            .compile("korri_android_log");
        println!("cargo:rustc-link-lib=log");
        println!("cargo:rerun-if-changed=src/android/log.c");
        // The core needs libcrypto, which openssl-sys links after it.
        println!("cargo:rustc-link-search=native={lib}");
        println!("cargo:rustc-link-lib=static=moonlight-common-c");
        println!("cargo:rustc-link-lib=static=enet");
        return;
    }
    let mut c = cc::Build::new();
    c.include("picture")
        .include(include)
        .define("HAVE_X11", None)
        .define("HAVE_VAAPI", None)
        .define("HAVE_SDL", None)
        .define("HAVE_ALSA", None)
        .define("HAVE_PULSE", None);
    if request {
        c.define("HAVE_V4L2M2M", None);
    }
    let handheld: &[&str] = if request { &["libdrm"] } else { &[] };
    for lib in [
        "libavcodec",
        "libavutil",
        "libva-x11",
        "x11",
        "egl",
        "glesv2",
        "sdl2",
        "opus",
        "alsa",
        "libpulse-simple",
        "libevdev",
        "libudev",
    ]
    .iter()
    .chain(handheld)
    {
        let library = pkg_config::Config::new().probe(lib).unwrap();
        for path in library.include_paths {
            c.include(path);
        }
    }
    for file in [
        "bridge.c",
        "frame_times.c",
        "sdl.c",
        "util.c",
        "video/x11.c",
        "video/sdl.c",
        "video/ffmpeg.c",
        "video/ffmpeg_vaapi.c",
        "video/egl.c",
    ] {
        c.file(format!("picture/{file}"));
    }
    if request {
        c.file("picture/video/v4l2m2m.c");
    }
    for file in [
        "bridge.c",
        "audio/alsa.c",
        "audio/pulse.c",
        "audio/sdl.c",
        "input/evdev.c",
        "input/udev.c",
        "input/mapping.c",
        "input/sdl.c",
        "input/x11.c",
    ] {
        c.file(format!("io/{file}"));
    }
    c.flag_if_supported("-Wno-unused-parameter")
        .opt_level(2)
        .compile("korri_picture");
    println!("cargo:rustc-link-search=native={lib}");
    println!("cargo:rustc-link-lib=moonlight-common-c");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rerun-if-changed=picture");
    println!("cargo:rerun-if-changed=io");
}
