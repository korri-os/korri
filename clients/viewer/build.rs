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
        // GetEstimatedRttInfo: Android's stream health (src/android/status.rs).
        .allowlist_function("Li(Initialize(ServerInformation|StreamConfiguration|ConnectionCallbacks)|StartConnection|StopConnection|InterruptConnection|GetLaunchUrlQueryParameters|GetMicroseconds|GetPendingAudioDuration|GetEstimatedRttInfo)")
        // The C core's input calls (src/network_core/moonlight_common_c.rs):
        // Android's input and Linux's carried C input reach them through
        // the network core seam.
        .allowlist_function("LiSend(KeyboardEvent2|MultiControllerEvent|ControllerArrivalEvent|MouseMoveEvent|MousePositionEvent|MouseButtonEvent|HighResScrollEvent|HighResHScrollEvent|TouchEvent|ControllerTouchEvent|ControllerMotionEvent)")
        // Its feedback (src/android/feedback.rs) reports each controller's
        // battery.
        .allowlist_function("LiSendControllerBatteryEvent")
        .allowlist_function("picture_.*|viewer_.*")
        .allowlist_type("(STREAM_CONFIGURATION|SERVER_INFORMATION|CONNECTION_LISTENER_CALLBACKS|DECODER_RENDERER_CALLBACKS|AUDIO_RENDERER_CALLBACKS|OPUS_MULTISTREAM_CONFIGURATION|DECODE_UNIT|LENTRY)")
        // Android's decoder (src/android/picture.rs) reads decode units; its
        // sound (src/android/sound.rs) names its capabilities.
        // nightjar-core's connection status (src/network_core/nightjar.rs).
        .allowlist_var("(VIDEO_FORMAT_.*|STREAM_CFG_.*|ENCFLG_.*|BUFFER_TYPE_.*|FRAME_TYPE_.*|DR_.*|CAPABILITY_.*|KEY_ACTION_.*|BUTTON_ACTION_.*|CONN_STATUS_.*)")
        .derive_default(true)
        .generate().expect("generate binding from the pinned core's actual header");
    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("network.rs"))
        .unwrap();
    // The core's log callback is C-variadic: logcat on Android, standard
    // output elsewhere, then the start's marks (src/marks.rs).
    cc::Build::new()
        .file("src/core_log.c")
        .compile("korri_core_log");
    println!("cargo:rerun-if-changed=src/core_log.c");
    if android {
        println!("cargo:rustc-link-lib=log");
        // The core needs libcrypto, which openssl-sys links after it.
        println!("cargo:rustc-link-search=native={lib}");
        println!("cargo:rustc-link-lib=static=moonlight-common-c");
        println!("cargo:rustc-link-lib=static=enet");
        // The sound's Opus decoder (src/android/audio.rs): the libopus
        // release that Linux links, built for Android by android.nix.
        let opus = env::var("OPUS_LIB").expect("missing Opus library path");
        println!("cargo:rerun-if-env-changed=OPUS_LIB");
        println!("cargo:rustc-link-search=native={opus}");
        println!("cargo:rustc-link-lib=static=opus");
        return;
    }
    let mut c = cc::Build::new();
    // The carried code's calls into the network core go through the seam
    // (src/network_core/c_calls.rs): this header renames them.
    let seam =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/network_core/c_calls.h");
    println!("cargo:rerun-if-changed={}", seam.display());
    c.flag("-include")
        .flag(seam.to_str().expect("a UTF-8 source path"));
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
    let mut includes = Vec::new();
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
            c.include(&path);
            includes.push(path);
        }
    }
    // The Rust audio output (#391 step F, src/audio_output/) calls libopus
    // and SDL's audio itself: bind the functions it uses from the headers of
    // the libraries linked above. pkg-config already links both.
    if env::var_os("CARGO_FEATURE_RUST_AUDIO_OUTPUT").is_some() {
        includes
            .iter()
            .fold(
                bindgen::Builder::default()
                    .header_contents("audio.h", "#include <opus_multistream.h>\n#include <SDL.h>\n"),
                |builder, path| builder.clang_arg(format!("-I{}", path.display())),
            )
            .allowlist_function("opus_multistream_(decoder_create|decode|decoder_destroy)")
            .allowlist_function("SDL_(InitSubSystem|OpenAudioDevice|PauseAudioDevice|QueueAudio|CloseAudioDevice|GetError)")
            .allowlist_type("SDL_AudioSpec")
            .allowlist_var("AUDIO_S16LSB|SDL_INIT_AUDIO|OPUS_OK|OPUS_APPLICATION_AUDIO")
            // SDL's comments hold indented examples that rustdoc would run.
            .generate_comments(false)
            .derive_default(true)
            .generate()
            .expect("generate the audio binding from the linked libraries' headers")
            .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("audio.rs"))
            .unwrap();
    }
    // The VA-API EGL presenter (#391 step F, src/egl_presenter/). With the
    // feature, Rust takes its two names and chooses at run time between its
    // own presenter and the carried C, which keeps its code under the
    // `carried_` names. The caller, video/ffmpeg_vaapi.c, does not change.
    let egl_presenter = env::var_os("CARGO_FEATURE_RUST_EGL_PRESENTER").is_some();
    const EGL_PRESENTERS: [&str; 1] = ["video/vaapi_egl.c"];
    const EGL_NAMES: [&str; 2] = ["vaapi_egl_present", "vaapi_egl_cleanup"];
    if egl_presenter {
        let mut carried = c.clone();
        for name in EGL_NAMES {
            carried.define(name, Some(format!("carried_{name}").as_str()));
        }
        for file in EGL_PRESENTERS {
            carried.file(format!("picture/{file}"));
        }
        carried
            .flag_if_supported("-Wno-unused-parameter")
            .opt_level(2)
            .compile("korri_carried_egl");
        // After that archive: the linker keeps a library only for the
        // references that come before it.
        println!("cargo:rustc-link-lib=EGL");
        println!("cargo:rustc-link-lib=GLESv2");
        // The types, names and constants that the Rust presenter reads, from
        // the same headers. The X11 calls serve the tests' windows.
        includes
            .iter()
            .fold(
                bindgen::Builder::default()
                    .header_contents(
                        "presenter.h",
                        "#define XUTIL_DEFINE_FUNCTIONS\n\
                         #include \"video/vaapi_egl.h\"\n\
                         #include <EGL/egl.h>\n\
                         #include <EGL/eglext.h>\n\
                         #include <GLES2/gl2.h>\n\
                         #include <GLES2/gl2ext.h>\n\
                         #include <X11/Xutil.h>\n",
                    )
                    .clang_arg("-Ipicture"),
                |builder, path| builder.clang_arg(format!("-I{}", path.display())),
            )
            .allowlist_type("AVFrame|VADRMPRIMESurfaceDescriptor|picture_place|XImage")
            .allowlist_function("picture_fit")
            .allowlist_function("av_frame_(alloc|free)")
            .allowlist_function("X(OpenDisplay|DefaultRootWindow|CreateSimpleWindow|MapWindow|DestroyWindow|Sync|GetImage|GetPixel|DestroyImage)")
            .allowlist_var("EGL_(LINUX_DMA_BUF_EXT|LINUX_DRM_FOURCC_EXT|DMA_BUF_PLANE[0-2]_(FD|OFFSET|PITCH|MODIFIER_LO|MODIFIER_HI)_EXT|YUV_COLOR_SPACE_HINT_EXT|SAMPLE_RANGE_HINT_EXT|ITU_REC(601|709|2020)_EXT|YUV_(FULL|NARROW)_RANGE_EXT)")
            .allowlist_var("GL_TEXTURE_EXTERNAL_OES|ZPixmap|VA_FOURCC_NV12")
            .generate_comments(false)
            .derive_default(true)
            .generate()
            .expect("generate the presenter binding from the picture's headers")
            .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("presenter.rs"))
            .unwrap();
    }
    for file in [
        "bridge.c",
        "frame_times.c",
        "place.c",
        "sdl.c",
        "util.c",
        "video/x11.c",
        "video/sdl.c",
        "video/ffmpeg.c",
        "video/ffmpeg_vaapi.c",
        "video/vaapi_egl.c",
    ]
    .into_iter()
    .filter(|file| !(egl_presenter && EGL_PRESENTERS.contains(file)))
    {
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
