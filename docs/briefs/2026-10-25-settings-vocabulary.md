# One settings vocabulary for runner, modifier and service plugins

**Status: draft for owner review, 2026-10-25.** Nothing here is implemented.
The cascade still stores flat scalar settings (`LaunchSettingValue`), so every
nested shape below needs the nested-settings change that the RetroArch README
already names as unresolved ("Supplying and persisting nested policy through
the host remains an unresolved integration boundary").

## Sources

| Source | What it contributes |
|---|---|
| `docs/briefs/2026-09-15-plugin-model/CASCADE.md` §2–3 | kebab-case user-facing names; nested sections; objects merge field by field |
| ADR 0028 | every Korri key in `device.yaml` is kebab-case; runner settings live in `host.runners.<id>.settings`, modifier settings in `host.modifiers.<id>.settings` |
| `korri-plugins/plugins/retroarch/policy.ts` (legacy RetroArch policy, unchanged) | the section names `video`, `audio`, `input`, `logging`, `saves`, `rewind`, …; units in names (`latencyMs`, `bufferSizeMb`, `volumeDb`, `maxNits`); `enable` inside a feature section (`rewind.enable`, `video.hdr.enable`) |
| `legacy:product/plugins/gamescope/src/launch-companion/policy.ts` | Gamescope sections `display`, `scaling`, `window`, `cursor`, `hdr`, `debug`, … |
| `legacy:product/plugins/moonlight/src/config/policy.ts` | Moonlight sections `stream`, `input`, `logging`; `bitrateKbps`, `packetSizeBytes` |
| `korri-plugins/plugins/gamescope/compose.ts` + `README.md` | the 38 typed Gamescope 3.16.31 options and the excluded ones |
| moonlight-embedded `775444287305849ebdf4736c75298ad0713e2d5d` `src/config.c`, `src/main.c` | the 32 upstream options (+2 from Korri patch 0013) |
| Sunshine `v2025.924.154138` `docs/configuration.md` | the 85 upstream options (+2 from Korri patches 0037/0038) |

RetroArch is the base vocabulary because it is the largest legacy policy. Its
names change only where a rule below says so.

## Rules

| # | Rule | Grounding |
|---|---|---|
| R1 | Every key is kebab-case. Choice values that Korri names are kebab-case; values that are the program's own names keep its spelling (`h264`, `PerAppId`). | CASCADE.md §2, ADR 0028 |
| R2 | Shared top-level sections, one meaning each: `video` (the picture a program makes, sends or shows), `audio`, `input`, `network` (links between devices), `logging` (diagnostic output), `debug` (switches for diagnosing the program, not for play). A plugin adds its own section only for a concept no other plugin has (`saves`, `rewind`, `cursor`, `window`, …). | RetroArch and Sunshine section names |
| R3 | A number with a unit ends with the unit: `-ms`, `-seconds`, `-hz`, `-kbps`, `-mb`, `-bytes`, `-nits`, `-db`, `-percent`, `-degrees`. | RetroArch `latencyMs`, `bufferSizeMb`, `volumeDb`; Moonlight `bitrateKbps` |
| R4 | A feature with its own settings is a section with `enable`. A lone switch is a positive boolean named for what `true` does, with no `-enable` suffix and no `no-` prefix. A concept shared by two plugins uses one form in both (`video.hdr.enable` everywhere). | RetroArch `rewind.enable`, `video.hdr.enable`; Gamescope `hdr.enable` |
| R5 | `width` and `height` are siblings in an object named for the surface: `resolution` when a program has one picture size, a named surface when it has more (`output`, `nested`). They are set together. | Gamescope pairing rule (`gamescope-setting-pair`) |
| R6 | Rates are three different terms: `fps` (frames a program makes or asks for), `fps-limit` (a cap), `refresh-rate-hz` (a display's refresh). | Gamescope `--help` calls `nested-refresh` "game refresh rate (frames per second)" |
| R7 | An enumeration is a named choice, never an integer code. | legacy Moonlight `MoonlightCodec`, Gamescope `GamescopeScaler` |
| R8 | Korri invariants never appear. Each plugin keeps its table of options with no typed setting. | Gamescope README, AGENTS.md plugin boundaries |

## Shared terms

| Term | Kind | Meaning | RetroArch | Gamescope | Moonlight | Sunshine |
|---|---|---|---|---|---|---|
| `video.resolution.{width,height}` | integer pair | size of the one picture the program makes or asks for | `fullscreenWidth/Height` | | `width`/`height` | |
| `video.output` / `video.nested` | surface objects | Gamescope's two surfaces | | `output-*`, `nested-*` | | |
| `video.fps` | integer | frames per second made or asked for | | `nested-refresh` | `fps` | |
| `video.fps-limit` | integer | cap, 0 = none | | `framerate-limit` | | |
| `video.minimum-fps` | integer | floor | | | | `minimum_fps_target` |
| `video.refresh-rate-hz` | number | display refresh the program assumes | `refreshRate` | | | |
| `video.bitrate-kbps` / `video.max-bitrate-kbps` | integer | asked-for bitrate / host cap | | | `bitrate` | `max_bitrate` |
| `video.codec` | choice | video codec | | | `codec` | |
| `video.hdr.enable` | boolean | HDR on | `hdr.enable` | `hdr-enabled` | `hdr` | |
| `video.scaling` | section | how a picture is fitted to a surface | see open item 1 | `scaler`, `filter`, `sharpness`, `max-scale` | | |
| `video.rotation-degrees` | choice 0/90/180/270 | picture rotation | | | `rotate` | |
| `audio.enable` | boolean | sound on | `enable` | | | `stream_audio` |
| `audio.channels` | choice | stereo / 5.1 / 7.1 | | | `surround` | |
| `input.mouse` | boolean | mouse input accepted | | | | `mouse` |
| `input.keyboard` | boolean | keyboard input accepted | | | | `keyboard` |
| `network.*` | section | links between devices | | | `remote`, `packetsize` | Network section |
| `logging.level` | choice | `none`, `fatal`, `error`, `warning`, `info`, `debug`, `verbose`; each plugin accepts its subset | see open item 3 | | `-verbose`, `-debug` | `min_log_level` |

## RetroArch (`host.runners.<core runner id>.settings`, family `@korri:retroarch`)

Every key of `policy.ts`, renamed only where a rule applies. `# was` names
the legacy key when it changed. Values are examples.

```yaml
settings:
  logging:
    level: debug                 # was verbose + verbosity; open item 3
    core-level: warning          # was libretroLogLevel
  lifecycle:
    save-on-exit: false          # reserved today (config_save_on_exit)
    auto-overrides: true
    auto-remaps: true
    game-specific-options: true
    auto-shaders: true
    show-hidden-files: false
    load-dummy-on-core-shutdown: false
    history-list: false          # was historyListEnable (R4)
    performance-counters: false
    all-users-control-menu: true
    suspend-screensaver: true
    sustained-performance-mode: false
    game-mode: false
  drivers:
    input: udev
    joypad: udev
    video: gl
    audio: pipewire
    resampler: sinc
    menu: ozone                  # reserved today (menu_driver)
    camera: null
    location: null
    record: ffmpeg
  video:
    fullscreen: true
    windowed-fullscreen: false
    resolution: { width: 640, height: 480 }   # was fullscreenWidth/Height (R5)
    refresh-rate-hz: 60.0        # was refreshRate (R3, R6)
    vsync: true
    aspect-ratio: core-provided  # config | square | core-provided | custom | full
    aspect-ratio-value: 1.333
    force-aspect: true
    scale: 3.0
    integer-scale: true          # open item 1
    crop-overscan: true
    smooth: false
    shader:                      # was shader + shaderEnable + shaderWatchFiles (R4)
      enable: true
      path: crt-easymode.glslp
      watch-files: false
    hdr:
      enable: false
      max-nits: 1000
      paper-white-nits: 200
      contrast: 5.0
      expand-gamut: false
    recording:
      post-filter: false
      gpu: false
    gpu-screenshot: true
    sync:
      hard-sync: false
      hard-sync-frames: 0
      frame-delay-ms: 0          # was frameDelay (R3; video_frame_delay is ms)
      frame-delay-auto: false
    stats:                       # was logging.fpsShow/memoryShow/framecountShow (R2)
      fps: true
      memory: false
      frame-count: false
  audio:
    enable: true
    menu-sounds: false           # was menuEnable (R4)
    mute: false
    mixer-mute: false
    latency-ms: 64
    output-rate-hz: 48000        # was outputRate (R3)
    sync: true
    rate-control:                # was rateControl + rateControlDelta (R4)
      enable: true
      delta: 0.005
    max-timing-skew: 0.05
    volume-db: 0.0
    mixer-volume-db: 0.0
    resampler-quality: normal
  input:
    autodetect: true
    max-users: 2
    poll-type-behavior: late
    axis-threshold: 0.5
    analog-deadzone: 0.1
    analog-sensitivity: 1.0
    remap-binds: true
    descriptors:
      label-show: true
      hide-unbound: false
    overlay:
      enable: false
      path: gamepads/neo-retropad/neo-retropad.cfg
      opacity: 0.7
      scale: 1.0
      behind-menu: true
      hide-in-menu: true
    auto-game-focus: detect      # integer code 0..2 -> off | on | detect (R7)
    menu-toggle-combo: l3-r3     # was menuToggleGamepadCombo
    quit-combo: none             # was quitGamepadCombo
    ports:
      '1':
        libretro-device: 1
        joypad-index: 0
        analog-dpad-mode: none
  menu:
    show-start-screen: false
    pause-core: true             # was pauseLibretro
    mouse: false                 # was mouseEnable (R4)
    pointer: true                # was pointerEnable
    clock: true                  # was timedateEnable
    battery-level: true          # was batteryLevelEnable
    core-name: true              # was coreEnable
    dynamic-wallpaper: false
    wallpaper: null
    screensaver-timeout-seconds: 0
  saves:
    autosave-interval-seconds: 0
    auto-load-state: true
    auto-save-state: true
    auto-index: false
    max-keep: 0
    thumbnail: true              # was thumbnailEnable (R4)
    sort-savefiles: false
    sort-savestates: false
    savefiles-in-content-dir: false     # open item 4
    savestates-in-content-dir: false    # open item 4
    systemfiles-in-content-dir: false   # open item 4
    block-sram-overwrite: false
    save-file-compression: false
    state-file-compression: true
  rewind:
    enable: false
    granularity: 1
    buffer-size-mb: 20
    buffer-size-step-mb: 10
    auto-stride: false
  playback:
    pause-when-inactive: true    # was pauseNonactive
    pause-on-disconnect: true
    slowmotion-ratio: 3.0
    fastforward-ratio: 0.0
    fastforward-frameskip: true
  latency:
    run-ahead:
      enable: false
      frames: 1
      secondary-instance: true
      hide-warnings: false
    preemptive-frames:
      enable: false
      frames: 1
  achievements:
    enable: false
    username: example
    hardcore: false              # was hardcoreMode
    badges: false
    rich-presence: true
    test-unofficial: false
  haptics:
    vibrate-on-keypress: false
    device-vibration: false
  playlists:
    use-old-format: false
  privacy:
    camera-device: null
    camera: false                # was cameraAllow
    location: false              # was locationAllow
  updater:
    show-online-updater: false
    show-core-updater: false
    buildbot-url: null           # open item 5
    buildbot-assets-url: null    # open item 5
    auto-extract-archive: true
```

Not carried over (R8): `environment` (the cascade's `env` kind holds it),
`configFile` (the cascade's `config` kind), `core.path` and `content.path`
(korrid supplies both), `logging.logFile` (a file path the plugin would write),
`audio.device` (Korri routes audio through PipeWire), and the `paths` section
(korrid manages `savefile_directory`, `savestate_directory`,
`screenshot_directory`, and the other paths point into Korri-owned storage).
`config_save_on_exit` and `menu_driver` stay reserved as today.

## Gamescope (`host.modifiers.'@korri:gamescope/gamescope'.settings`)

All 38 typed keys of `compose.ts`, in shared sections first.

```yaml
enabled: true
settings:
  video:
    output: { width: 640, height: 480 }                # output-width/height
    nested: { width: 320, height: 240, fps: 60 }       # nested-*; nested-refresh -> fps (R6)
    fps-limit: 30                                      # framerate-limit (R6)
    adaptive-sync: true
    force-composition: true                            # was debug in legacy; on by default for pacing
    color-management: true                             # disable-color-management, inverted (R4)
    scaling:
      scaler: integer                                  # auto | integer | fit | fill | stretch
      filter: nearest                                  # linear | nearest | fsr | nis | pixel
      sharpness: 5
      max: 2.0                                         # max-scale
    hdr:
      enable: true                                     # hdr-enabled
      sdr-content-nits: 400
      sdr-gamut-wideness: 0.5
      inverse-tone-mapping:
        enable: true                                   # hdr-itm-enabled
        sdr-nits: 100
        target-nits: 1000
    reshade:
      effect: crt.fx
      technique-index: 0                               # reshade-technique-idx
  input:
    mouse-sensitivity: 1.5
    default-touch-mode: trackpad                       # 0..6 -> hover | left | right | middle | passthrough | disabled | trackpad (R7)
  window:
    borderless: true
    grab-cursor: true                                  # force-grab-cursor
    force-windows-fullscreen: true
    xwayland-count: 2
    fade-out-ms: 200                                   # fade-out-duration (R3)
  cursor:
    image: /var/lib/korri/cursor.png                   # cursor
    hotspot: '4,4'                                     # cursor-hotspot
    scale-height: 720                                  # cursor-scale-height
    hide-delay-ms: 3000                                # hide-cursor-delay (R3)
  backend:
    allow-deferred: true                               # allow-deferred-backend
    prefer-vk-device: '1002:73bf'
  scheduling:
    realtime: true                                     # rt
  steam:
    enable: true                                       # steam
    virtual-connector-strategy: PerAppId               # legacy: embedded.virtualConnectorStrategy
  debug:
    xres: true                                         # disable-xres, inverted (R4)
```

The README's table of options with no typed setting stays as is.

## Moonlight (`host.runners.'@korri:moonlight/moonlight'.settings`)

All 16 typed options of the earlier draft, as 15 keys (`-verbose` and
`-debug` become one level).

```yaml
settings:
  video:
    resolution: { width: 854, height: 480 }   # -width, -height; unset: display size
    fps: 30                                   # -fps; unset: Moonlight's 60
    bitrate-kbps: 5000                        # -bitrate; unset: Moonlight picks from size and fps
    codec: h265                               # -codec: auto | h264 | h265 | av1; unset: Korri's h264
    hdr:
      enable: true                            # -hdr (R4: shared form)
    rotation-degrees: 90                      # -rotate
    optimize-game-settings: false             # -nosops, inverted (R4)
    allow-unsupported-resolution: true        # -nounsupported, inverted (R4)
  audio:
    channels: '5.1'                           # -surround; stereo | 5.1 | 7.1
    play-on-host: true                        # -localaudio
  input:
    mouse-emulation: false                    # -nomouseemulation, inverted (R4)
  network:
    remote-optimizations: auto                # -remote: auto | on | off (legacy remoteOptimizations)
    packet-size-bytes: 1024                   # -packetsize
  logging:
    level: debug                              # info | verbose | debug (-verbose, -debug)
```

Not carried over (R8): `720`, `1080`, `4k`, `app`, `input`, `viewonly`,
`mapping`, `keydir`, `pin`, `port`, `platform`, `windowed`,
`autowindowresize`, `noautowindowresize`, `quitappafter`, `audio`, `config`,
`save`. Legacy's `command`, `environment`, `platform`, `input.devices`,
`input.mappingFile`, `input.viewOnly`, `input.touch`, `audio.device`,
`window.*`, `control.*`, `stream.keyDir`, `stream.quitAppAfter` and
`extraArgs` stay out for the same reasons. Legacy ranges (min/start/max)
are dropped: nothing on `main` changes these values during a stream.

## Sunshine (home unresolved)

`device.yaml` has no section for a service plugin's settings
(`DEVICE_SECTIONS`: `host`, `storage`, `providers`, `systems`, `routines`,
`locations`, `sources`, `provider-links`). Sunshine reads its settings only
when it starts, and restarting `korri-sunshine` ends the session that runs on
the host (observed on the RG35XX SP, 2026-10-25). 31 typed keys:

```yaml
settings:
  logging:
    level: debug                              # min_log_level
  input:
    controller: true
    gamepad: ds5                              # auto | ds5 | switch | xone
    motion-as-ds4: false
    touchpad-as-ds4: false
    ds5-random-mac: false                     # ds5_inputtino_randomize_mac
    back-button-timeout-ms: 2000              # -1 = no Guide emulation
    keyboard: true
    key-repeat:
      delay-ms: 400                           # key_repeat_delay
      rate-hz: 30.0                           # key_repeat_frequency (R3)
    right-alt-as-windows-key: true            # key_rightalt_to_key_win
    mouse: false
    high-resolution-scrolling: false
    pen-touch: false                          # native_pen_touch
  audio:
    enable: true                              # stream_audio (R4: shared form)
  video:
    max-bitrate-kbps: 3000                    # max_bitrate
    minimum-fps: 30                           # minimum_fps_target
    encoder:
      name: v4l2m2m                           # encoder; v4l2m2m, rkmpp from patches 0021/0022
      fec-percent: 10                         # fec_percentage (R3)
      qp: 28
      min-threads: 2
      hevc: main                              # hevc_mode 0..3 -> auto | off | main | main10 (R7)
      av1: off                                # av1_mode 0..3, same names
      software:
        preset: superfast                     # sw_preset
        tune: zerolatency                     # sw_tune
    capture:
      vblank-offset-ms: 2                     # capture_vblank_offset_ms (patch 0037)
  network:
    address-family: both                      # ipv4 | both
    external-ip: 203.0.113.7
    lan-encryption: required                  # 0 | 1 | 2 -> off | supported | required (R7)
    wan-encryption: supported
    ping-timeout-ms: 15000
```

Not carried over (R8), 56 options: Korri files and identity (`file_apps`,
`credentials_file`, `pkey`, `cert`, `file_state`, `log_path`); set by the
plugin (`capture`, `capture_rt_priority`); Korri chooses the device
(`adapter_name`, `output_name`); PipeWire routing (`audio_sink`,
`virtual_sink`); network exposure owned by the plugin's firewall rules
(`port`, `upnp`, `origin_web_ui_allowed`); runs commands (`global_prep_cmd`);
Korri's device label (`sunshine_name`); no UI on the device (`locale`,
`system_tray`, `notify_pre_releases`); Windows only
(`always_send_scancodes`, `install_steam_audio_drivers`,
`ds4_back_as_touchpad_click`, ten `dd_*`); hardware no Korri device has,
inferred and not checked per SoC (`nvenc_*` ×8, `qsv_*` ×3, `amd_*` ×7,
`vt_*` ×3, `vaapi_strict_rc_buffer`); no list kind yet (`keybindings`).

## Open items

1. **Scaling.** RetroArch has `video.scale` and `video.integer-scale`;
   Gamescope has `video.scaling.scaler: integer`. They describe the same idea
   in two forms. This draft leaves both until one form is chosen.
2. **Nested settings in the cascade.** The fold, the editor's rows and the
   wire type need nesting before any section above can be stored.
3. **RetroArch logging.** Legacy has `verbose` (a command-line flag, not
   ported) and `verbosity` (`log_verbosity`). Merging them into
   `logging.level` needs the legacy argv renderer checked first.
4. **`*-in-content-dir`.** These move saves out of Korri's managed save
   locations. They are probably Korri-owned like the save directories.
5. **Updater URLs.** `buildbot-url` and `buildbot-assets-url` choose where
   RetroArch downloads from. They may belong with the other Korri-owned
   network exposure.
