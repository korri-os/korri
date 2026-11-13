// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, path::PathBuf};
fn main() {
    let include =
        env::var("MOONLIGHT_COMMON_INCLUDE").expect("use the viewer Nix development shell");
    let lib = env::var("MOONLIGHT_COMMON_LIB").expect("missing network library path");
    let bindings = bindgen::Builder::default()
        .header("picture/bridge.h")
        .header("io/bridge.h")
        .clang_arg(format!("-I{include}"))
        .allowlist_function("Li(Initialize(ServerInformation|StreamConfiguration|ConnectionCallbacks)|StartConnection|StopConnection|GetLaunchUrlQueryParameters)")
        .allowlist_function("picture_.*|viewer_.*")
        .allowlist_type("(STREAM_CONFIGURATION|SERVER_INFORMATION|CONNECTION_LISTENER_CALLBACKS|DECODER_RENDERER_CALLBACKS|AUDIO_RENDERER_CALLBACKS)")
        .allowlist_var("(VIDEO_FORMAT_.*|STREAM_CFG_.*|ENCFLG_.*)")
        .derive_default(true)
        .generate().expect("generate binding from the pinned core's actual header");
    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("network.rs"))
        .unwrap();
    let mut c = cc::Build::new();
    c.include("picture")
        .include(include)
        .define("HAVE_X11", None)
        .define("HAVE_VAAPI", None)
        .define("HAVE_SDL", None)
        .define("HAVE_ALSA", None)
        .define("HAVE_PULSE", None);
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
    ] {
        let library = pkg_config::Config::new().probe(lib).unwrap();
        for path in library.include_paths {
            c.include(path);
        }
    }
    for file in [
        "bridge.c",
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
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_INCLUDE");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_LIB");
}
