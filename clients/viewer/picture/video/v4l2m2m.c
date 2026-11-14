/*
 * This file is part of Moonlight Embedded.
 *
 * V4L2 M2M hardware-decode backend for Qualcomm SM8550
 * (Adreno 740 + iris VPU + freedreno Mesa).
 *
 * Shipping pipeline:
 *
 *   FFmpeg `hevc_v4l2m2m` / `h264_v4l2m2m`  --[iris VPU]-->
 *   AVFrame { format = AV_PIX_FMT_NV12 } -->
 *   SDL_UpdateNVTexture() -->
 *   SDL_RenderCopy() / SDL_RenderPresent()
 *
 * This keeps the expensive HEVC/H.264 decode on the iris VPU while using
 * SDL's renderer for presentation. SDL owns Wayland window sizing,
 * compositor scale, live resize, and display moves, which matters for
 * devices like AYN Thor where the window may appear on either screen or
 * be resized while streaming.
 *
 * This is not currently true zero-copy: FFmpeg 8.0's v4l2_m2m decoder
 * wrapper advertises `capture=NV12/drm_prime` when asked for DRM_PRIME,
 * but v4l2_try_start() then overwrites avctx->pix_fmt with the native
 * V4L2 capture format (NV12) after VIDIOC_G_FMT. In practice, the iris
 * path emits CPU-visible NV12 frames. The upload cost is about 85 MB/s
 * at 720p60, which measured as a practical win anyway: ~49% process CPU
 * for the software SDL baseline vs ~13% for this VPU-backed path on Sobo.
 *
 * The dormant DRM_PRIME/EGL helpers remain for now as scaffolding for a
 * follow-up zero-copy experiment (ffmpeg_drm or direct V4L2/VIDIOC_EXPBUF),
 * but the active presentation path is SDL NV12.
 */
#include "video.h"
#include "../util.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdint.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <pthread.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <poll.h>
#include <linux/videodev2.h>
#include <inttypes.h>

#include <libavcodec/avcodec.h>
#include <libavutil/avutil.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_drm.h>
#include <libavutil/pixdesc.h>
#include <libavutil/pixfmt.h>

#include <drm_fourcc.h>

#include <SDL.h>

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl3.h>
#include <GLES2/gl2ext.h>

/* Single-frame display queue. PR #932's ffmpeg_drm.c uses the same
 * shape: hold one AVFrame* with av_frame_clone, drop the previous one
 * on overrun so vsync can never starve the decode pipeline. */
static AVCodec       *codec     = NULL;
static AVCodecContext *codec_ctx = NULL;
static AVPacket      *pkt       = NULL;
static void          *nal_buf   = NULL;
static size_t        nal_size   = 0;

static bool direct_dmabuf_present = false;

/* v4l2request platform: FFmpeg's software h264/hevc decoder drives a
 * stateless V4L2 decoder (Allwinner Cedrus, Rockchip Hantro) through the
 * V4L2 Request API hwaccel and emits AV_PIX_FMT_DRM_PRIME frames. The
 * display thread imports each capture buffer's Y and UV planes into the
 * SDL window's GLES context; no frame touches the CPU. */
static bool request_mode = false;

#define PRIME_CACHE_SIZE 32

typedef struct PrimeCacheEntry {
  int fd;
  int width;
  int height;
  int pitch;
  int uv_offset;
  EGLImageKHR img_y;
  EGLImageKHR img_uv;
  GLuint tex_y;
  GLuint tex_uv;
} PrimeCacheEntry;

static PrimeCacheEntry prime_cache[PRIME_CACHE_SIZE];
static int prime_cache_count = 0;
static const void *prime_cache_frames = NULL;

static int direct_quad_view_w = 0;
static int direct_quad_view_h = 0;
static int direct_quad_dst_x = 0;
static int direct_quad_dst_y = 0;
static int direct_quad_dst_w = 0;
static int direct_quad_dst_h = 0;
static int direct_quad_import_h = 0;
static int direct_quad_visible_h = 0;

static SDL_Window    *window    = NULL;
static SDL_GLContext  gl_ctx    = NULL;   /* DRM_PRIME path only (inactive on iris/FFmpeg v4l2m2m) */
static SDL_Renderer  *sdl_renderer = NULL;
static SDL_Rect       sdl_dst_rect = {0, 0, 0, 0};
static EGLDisplay     egl_dpy   = EGL_NO_DISPLAY;

/* GL state -- created on the display thread, after it takes the GL
 * context current. Touching these outside the display thread is a bug. */
static GLuint         gl_program = 0;     /* DRM_PRIME path: samplerExternalOES program */
static GLuint         gl_texture = 0;     /* DRM_PRIME path: external texture */
static GLuint         gl_vbo     = 0;     /* shared fullscreen-quad VBO */
static GLint          gl_uloc_tex = -1;

/* NV12 fallback GL state (Plan A). FFmpeg's hevc_v4l2m2m wrapper
 * unconditionally overrides av_pix_fmt with the V4L2 native pixel
 * format in v4l2_try_start(): see libavcodec/v4l2_m2m_dec.c line ~50
 * of v4l2_try_start, "avctx->pix_fmt = ff_v4l2_format_v4l2_to_avfmt(...)".
 * That means our DRM_PRIME preference is silently discarded and the
 * wrapper emits plain NV12 from MMAP'd buffers. We accept that here and
 * upload Y + UV planes to GL textures, sampling them in a BT.709
 * YUV->RGB fragment shader. The iris VPU still does HEVC decode -- we
 * pay only the upload cost (~85 MB/s @ 720p60).
 *
 * The DRM_PRIME path above is kept compiled-in for two reasons:
 *   (1) On a future FFmpeg or a future direct-V4L2 path (Plan C in the
 *       project journal), DRM_PRIME frames may start arriving and we
 *       want the zero-copy path to engage automatically without code
 *       changes.
 *   (2) draw_frame() branches at runtime on frame->format; the
 *       dead-for-now DRM_PRIME path stays exercised by the type
 *       checker and the linker, so bit-rot is contained. */
static GLuint         gl_program_nv12 = 0;
static GLuint         gl_texture_y    = 0;
static GLuint         gl_texture_uv   = 0;
static GLint          gl_uloc_y_nv12  = -1;
static GLint          gl_uloc_uv_nv12 = -1;
static GLint          gl_uloc_range_nv12 = -1;
static GLint          gl_uloc_coef_nv12 = -1;
static int            gl_nv12_colorspace = -1;
static int            gl_nv12_full_range = -1;
/* Colorspace of the decode units the host sends (COLORSPACE_REC_*). */
static volatile int   stream_colorspace = COLORSPACE_REC_601;

/* Load the YUV->RGB matrix and range into the bound NV12 program. */
static void set_nv12_color(int colorspace, bool full_range) {
  if (colorspace == gl_nv12_colorspace && (int) full_range == gl_nv12_full_range)
    return;
  if (full_range)
    glUniform3f(gl_uloc_range_nv12, 0.0f, 1.0f, 1.0f);
  else
    glUniform3f(gl_uloc_range_nv12, 16.0f / 255.0f, 255.0f / 219.0f, 255.0f / 224.0f);
  if (colorspace == COLORSPACE_REC_601)
    glUniform4f(gl_uloc_coef_nv12, 1.402f, 0.344136f, 0.714136f, 1.772f);
  else
    glUniform4f(gl_uloc_coef_nv12, 1.5748f, 0.1873f, 0.4681f, 1.8556f);
  gl_nv12_colorspace = colorspace;
  gl_nv12_full_range = full_range;
}
static int            gl_nv12_width   = 0;
static int            gl_nv12_height  = 0;
static bool           gl_nv12_inited  = false;

/* Presentation state. The decoded stream size and the compositor-visible
 * window size are independent: on Sobo the stream is 1280x720 while sway's
 * logical output is 960x540, and on Thor the window may move between
 * differently-sized/scaled displays. Recompute viewport + quad whenever
 * SDL reports a resize/display move. */
static int            gl_viewport_w   = 0;
static int            gl_viewport_h   = 0;
static int            gl_video_w      = 0;
static int            gl_video_h      = 0;

/* eglCreateImageKHR / eglDestroyImageKHR /
 * glEGLImageTargetTexture2DOES are extension entry points that must be
 * resolved at runtime via eglGetProcAddress. */
static PFNEGLCREATEIMAGEKHRPROC          egl_create_image  = NULL;
static PFNEGLDESTROYIMAGEKHRPROC         egl_destroy_image = NULL;
static PFNGLEGLIMAGETARGETTEXTURE2DOESPROC gl_image_target_texture = NULL;

static pthread_t       disp_thread;
static pthread_mutex_t disp_mtx  = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t  disp_cond = PTHREAD_COND_INITIALIZER;
static AVFrame        *disp_pending = NULL;   /* owned by display thread once dequeued */
static bool            disp_eos     = false;
static bool            disp_ready   = false;  /* GL setup completed */
static int             disp_setup_rc = 0;     /* propagated to setup() return */

/* Logging shim that matches the prefix ffmpeg_drm uses, so a single grep
 * across launch.log surfaces both platforms in chronological order
 * during an A/B (see guest/launchers/remote-moonlight-runtime-ab.sh). */
#define LOG(fmt, ...) fprintf(stderr, "v4l2m2m: " fmt "\n", ##__VA_ARGS__)

/* ------------------------------------------------------------------ */
/* FFmpeg integration                                                 */
/* ------------------------------------------------------------------ */

static enum AVPixelFormat get_request_fmt(AVCodecContext *ctx,
                                          const enum AVPixelFormat *fmts) {
  (void) ctx;
  for (const enum AVPixelFormat *p = fmts; *p != AV_PIX_FMT_NONE; p++) {
    if (*p == AV_PIX_FMT_DRM_PRIME)
      return *p;
  }
  /* No V4L2 Request hwaccel for this stream. Refuse rather than decode
   * in software: this platform only presents dma-buf frames, and a
   * silent software fallback would hide the failure. */
  LOG("v4l2request: decoder did not offer AV_PIX_FMT_DRM_PRIME -- "
      "no capable stateless V4L2 decoder for this stream");
  return AV_PIX_FMT_NONE;
}

static int request_decoder_setup(int videoFormat, int width, int height) {
  enum AVCodecID id;
  if (videoFormat & VIDEO_FORMAT_MASK_H264)
    id = AV_CODEC_ID_H264;
  else if (videoFormat & VIDEO_FORMAT_MASK_H265)
    id = AV_CODEC_ID_HEVC;
  else {
    LOG("v4l2request: unsupported video format 0x%x", videoFormat);
    return -1;
  }
  codec = (AVCodec *) avcodec_find_decoder(id);
  if (!codec)
    return -1;
  codec_ctx = avcodec_alloc_context3(codec);
  if (!codec_ctx)
    return -1;
  codec_ctx->flags       |= AV_CODEC_FLAG_LOW_DELAY | AV_CODEC_FLAG_OUTPUT_CORRUPT;
  codec_ctx->flags2      |= AV_CODEC_FLAG2_SHOW_ALL;
  codec_ctx->thread_count = 1;
  codec_ctx->width        = width;
  codec_ctx->height       = height;
  codec_ctx->get_format   = get_request_fmt;

  AVBufferRef *hw_dev = NULL;
  int err = av_hwdevice_ctx_create(&hw_dev, AV_HWDEVICE_TYPE_V4L2REQUEST,
                                   NULL, NULL, 0);
  if (err < 0) {
    LOG("v4l2request: av_hwdevice_ctx_create failed: %s", av_err2str(err));
    avcodec_free_context(&codec_ctx);
    return -1;
  }
  codec_ctx->hw_device_ctx = hw_dev;

  if (avcodec_open2(codec_ctx, codec, NULL) < 0) {
    LOG("avcodec_open2 failed for %s", codec->name);
    avcodec_free_context(&codec_ctx);
    return -1;
  }
  pkt = av_packet_alloc();
  if (!pkt) {
    avcodec_free_context(&codec_ctx);
    return -1;
  }
  LOG("setup: v4l2request hwaccel on FFmpeg %s decoder", codec->name);
  return 0;
}

/* ------------------------------------------------------------------ */
/* GL setup (runs on the display thread)                              */
/* ------------------------------------------------------------------ */

static const char *vertex_shader_src =
  "#version 300 es\n"
  "in vec2 a_pos;\n"
  "in vec2 a_uv;\n"
  "out vec2 v_uv;\n"
  "void main() {\n"
  "  v_uv = a_uv;\n"
  "  gl_Position = vec4(a_pos, 0.0, 1.0);\n"
  "}\n";

static const char *fragment_shader_src =
  "#version 300 es\n"
  "#extension GL_OES_EGL_image_external_essl3 : require\n"
  "precision mediump float;\n"
  "uniform samplerExternalOES u_tex;\n"
  "in vec2 v_uv;\n"
  "out vec4 fragColor;\n"
  "void main() {\n"
  "  fragColor = texture(u_tex, v_uv);\n"
  "}\n";

static const char *fragment_shader_src_2d =
  "#version 300 es\n"
  "precision mediump float;\n"
  "uniform sampler2D u_tex;\n"
  "in vec2 v_uv;\n"
  "out vec4 fragColor;\n"
  "void main() {\n"
  "  fragColor = texture(u_tex, v_uv);\n"
  "}\n";

/* NV12 path shaders. Vertex shader is identical to the DRM_PRIME one
 * (just outputs uv), but inlined separately so each path stays
 * self-contained for grep-ability. */
static const char *vertex_shader_src_nv12 =
  "#version 300 es\n"
  "in vec2 a_pos;\n"
  "in vec2 a_uv;\n"
  "out vec2 v_uv;\n"
  "void main() {\n"
  "  v_uv = a_uv;\n"
  "  gl_Position = vec4(a_pos, 0.0, 1.0);\n"
  "}\n";

/* NV12 fragment shader: sample Y from u_tex_y (GL_R8) and UV from
 * u_tex_uv (GL_RG8 at half resolution), apply BT.709 limited-range
 * YUV->RGB. Coefficients from ITU-R BT.709 Annex B; using limited-range
 * scaling because Sunshine encodes that way by default (Y in [16,235],
 * UV in [16,240]). */
static const char *fragment_shader_src_nv12 =
  "#version 300 es\n"
  "precision mediump float;\n"
  "uniform sampler2D u_tex_y;\n"
  "uniform sampler2D u_tex_uv;\n"
  "uniform vec3 u_range;\n"
  "uniform vec4 u_coef;\n"
  "in vec2 v_uv;\n"
  "out vec4 fragColor;\n"
  "void main() {\n"
  "  float y  = texture(u_tex_y,  v_uv).r;\n"
  "  vec2  uv = texture(u_tex_uv, v_uv).rg - vec2(0.5);\n"
  "  // BT.709 limited range expansion:\n"
  "  //   Y' = (Y - 16/255) * (255/219)\n"
  "  //   Cb = (Cb - 128/255) * (255/224) -- the - 128/255 is the\n"
  "  //   - vec2(0.5) above (we sampled 8-bit GL_RG into [0,1]).\n"
  "  // u_range = (Y offset, Y scale, chroma scale); u_coef = (R/Cr,\n"
  "  // G/Cb, G/Cr, B/Cb). gl_init_nv12 sets BT.709 limited range.\n"
  "  float yp = (y - u_range.x) * u_range.y;\n"
  "  float cb = uv.x * u_range.z;\n"
  "  float cr = uv.y * u_range.z;\n"
  "  vec3  rgb;\n"
  "  rgb.r = yp + u_coef.x * cr;\n"
  "  rgb.g = yp - u_coef.y * cb - u_coef.z * cr;\n"
  "  rgb.b = yp + u_coef.w * cb;\n"
  "  fragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0);\n"
  "}\n";

static GLuint compile_shader(GLenum type, const char *src) {
  GLuint s = glCreateShader(type);
  glShaderSource(s, 1, &src, NULL);
  glCompileShader(s);
  GLint ok = 0;
  glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
  if (!ok) {
    char log[1024] = {0};
    glGetShaderInfoLog(s, sizeof log - 1, NULL, log);
    LOG("shader compile failed: %s", log);
    glDeleteShader(s);
    return 0;
  }
  return s;
}

static int gl_init(int viewport_w, int viewport_h) {
  /* Resolve EGL/GL extension entry points. eglGetProcAddress is the
   * only portable path for these. */
  egl_create_image  = (PFNEGLCREATEIMAGEKHRPROC) eglGetProcAddress("eglCreateImageKHR");
  egl_destroy_image = (PFNEGLDESTROYIMAGEKHRPROC) eglGetProcAddress("eglDestroyImageKHR");
  gl_image_target_texture =
    (PFNGLEGLIMAGETARGETTEXTURE2DOESPROC) eglGetProcAddress("glEGLImageTargetTexture2DOES");
  if (!egl_create_image || !egl_destroy_image || !gl_image_target_texture) {
    LOG("eglGetProcAddress failed for one of "
        "eglCreateImageKHR / eglDestroyImageKHR / glEGLImageTargetTexture2DOES");
    return -1;
  }

  /* Verify the EGL extensions are actually advertised by the driver.
   * The proc resolution above can succeed (function is in libEGL) even
   * when the display's extension string omits dma-buf support, but the
   * actual import will then EGL_BAD_ACCESS at runtime. */
  const char *exts = eglQueryString(egl_dpy, EGL_EXTENSIONS);
  if (!exts || !strstr(exts, "EGL_EXT_image_dma_buf_import")) {
    LOG("EGL_EXT_image_dma_buf_import not advertised by driver");
    return -1;
  }

  GLuint vs = compile_shader(GL_VERTEX_SHADER, vertex_shader_src);
  GLuint fs = compile_shader(GL_FRAGMENT_SHADER,
                             direct_dmabuf_present ? fragment_shader_src_2d : fragment_shader_src);
  if (!vs || !fs) {
    if (vs) glDeleteShader(vs);
    if (fs) glDeleteShader(fs);
    return -1;
  }
  gl_program = glCreateProgram();
  glAttachShader(gl_program, vs);
  glAttachShader(gl_program, fs);
  /* Bind attrib locations BEFORE link so they don't depend on the
   * driver's choice. Keep these in sync with the vertex shader. */
  glBindAttribLocation(gl_program, 0, "a_pos");
  glBindAttribLocation(gl_program, 1, "a_uv");
  glLinkProgram(gl_program);
  glDeleteShader(vs);
  glDeleteShader(fs);
  GLint linked = 0;
  glGetProgramiv(gl_program, GL_LINK_STATUS, &linked);
  if (!linked) {
    char log[1024] = {0};
    glGetProgramInfoLog(gl_program, sizeof log - 1, NULL, log);
    LOG("program link failed: %s", log);
    return -1;
  }
  gl_uloc_tex = glGetUniformLocation(gl_program, "u_tex");

  /* Fullscreen-quad VBO: positions (clip space) interleaved with UVs.
   * GLES2 has no VAO; rebind attribs every frame in draw(). */
  static const GLfloat quad[] = {
    /* x      y      u     v */
    -1.0f, -1.0f,  0.0f, 1.0f,
     1.0f, -1.0f,  1.0f, 1.0f,
    -1.0f,  1.0f,  0.0f, 0.0f,
     1.0f,  1.0f,  1.0f, 0.0f,
  };
  glGenBuffers(1, &gl_vbo);
  glBindBuffer(GL_ARRAY_BUFFER, gl_vbo);
  glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_DYNAMIC_DRAW);

  GLenum tex_target = direct_dmabuf_present ? GL_TEXTURE_2D : GL_TEXTURE_EXTERNAL_OES;
  LOG("gl_init: GL_VERSION=%s GL_RENDERER=%s dmabuf_target=%s",
      glGetString(GL_VERSION), glGetString(GL_RENDERER),
      tex_target == GL_TEXTURE_2D ? "2D" : "EXTERNAL_OES");
  glGenTextures(1, &gl_texture);
  glBindTexture(tex_target, gl_texture);
  glTexParameteri(tex_target, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
  glTexParameteri(tex_target, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
  glTexParameteri(tex_target, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
  glTexParameteri(tex_target, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

  glViewport(0, 0, viewport_w, viewport_h);
  glClearColor(0.0f, 0.0f, 0.0f, 1.0f);

  LOG("gl_init: program=%u tex=%u viewport=%dx%d ext_ok",
      gl_program, gl_texture, viewport_w, viewport_h);
  return 0;
}

static void pump_sdl_window_events(void) {
  SDL_Event event;
  while (SDL_PollEvent(&event)) {
    if (event.type == SDL_WINDOWEVENT) {
      switch (event.window.event) {
      case SDL_WINDOWEVENT_SIZE_CHANGED:
      case SDL_WINDOWEVENT_RESIZED:
      case SDL_WINDOWEVENT_DISPLAY_CHANGED:
        /* The next draw_frame() will call update_render_geometry() and
         * pick up the new drawable/window size. Keep the log compact but
         * useful for Thor multi-display bring-up. */
        LOG("window event: type=%u data=%d,%d display=%d",
            event.window.event, event.window.data1, event.window.data2,
            SDL_GetWindowDisplayIndex(window));
        gl_viewport_w = 0;
        gl_viewport_h = 0;
        break;
      default:
        break;
      }
    }
  }
}

static void update_render_geometry(int video_w, int video_h) {
  int out_w = 0, out_h = 0;
  if (sdl_renderer) {
    SDL_GetRendererOutputSize(sdl_renderer, &out_w, &out_h);
  } else if (gl_ctx) {
    SDL_GL_GetDrawableSize(window, &out_w, &out_h);
  }
  if (out_w <= 0 || out_h <= 0) {
    SDL_GetWindowSize(window, &out_w, &out_h);
  }
  if (out_w <= 0 || out_h <= 0)
    return;

  if (out_w == gl_viewport_w && out_h == gl_viewport_h &&
      video_w == gl_video_w && video_h == gl_video_h)
    return;

  gl_viewport_w = out_w;
  gl_viewport_h = out_h;
  gl_video_w = video_w;
  gl_video_h = video_h;

  SDL_Rect dst = {0, 0, out_w, out_h};
  if (video_w > 0 && video_h > 0) {
    float video_aspect = (float) video_w / (float) video_h;
    float out_aspect = (float) out_w / (float) out_h;
    if (out_aspect > video_aspect) {
      dst.w = (int) ((float) out_h * video_aspect + 0.5f);
      dst.x = (out_w - dst.w) / 2;
    } else if (out_aspect < video_aspect) {
      dst.h = (int) ((float) out_w / video_aspect + 0.5f);
      dst.y = (out_h - dst.h) / 2;
    }
  }
  sdl_dst_rect = dst;

  if (gl_ctx) {
    float left   = ((float) dst.x / (float) out_w) * 2.0f - 1.0f;
    float right  = ((float) (dst.x + dst.w) / (float) out_w) * 2.0f - 1.0f;
    float top    = 1.0f - ((float) dst.y / (float) out_h) * 2.0f;
    float bottom = 1.0f - ((float) (dst.y + dst.h) / (float) out_h) * 2.0f;
    GLfloat quad[] = {
      left,  bottom, 0.0f, 1.0f,
      right, bottom, 1.0f, 1.0f,
      left,  top,    0.0f, 0.0f,
      right, top,    1.0f, 0.0f,
    };
    glViewport(0, 0, out_w, out_h);
    glBindBuffer(GL_ARRAY_BUFFER, gl_vbo);
    glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_DYNAMIC_DRAW);
  }

  LOG("presentation(%s): video=%dx%d output=%dx%d dst=%d,%d %dx%d display=%d",
      gl_ctx ? "direct-dmabuf" : "SDL",
      video_w, video_h, out_w, out_h,
      dst.x, dst.y, dst.w, dst.h,
      SDL_GetWindowDisplayIndex(window));
}
/* Lazy NV12 GL setup. Called on the first frame whose pix_fmt is NV12
 * (and on resolution changes). Keeps the DRM_PRIME-path GL state
 * untouched so a future zero-copy run doesn't need a teardown. */
static int gl_init_nv12(int width, int height) {
  if (gl_nv12_inited && gl_nv12_width == width && gl_nv12_height == height)
    return 0;

  /* Tear down prior state if the resolution changed mid-stream
   * (Sunshine renegotiates on display reconfig). */
  if (gl_nv12_inited) {
    if (gl_program_nv12) { glDeleteProgram(gl_program_nv12); gl_program_nv12 = 0; }
    if (gl_texture_y)    { glDeleteTextures(1, &gl_texture_y); gl_texture_y = 0; }
    if (gl_texture_uv)   { glDeleteTextures(1, &gl_texture_uv); gl_texture_uv = 0; }
    gl_nv12_inited = false;
  }

  GLuint vs = compile_shader(GL_VERTEX_SHADER,   vertex_shader_src_nv12);
  GLuint fs = compile_shader(GL_FRAGMENT_SHADER, fragment_shader_src_nv12);
  if (!vs || !fs) {
    if (vs) glDeleteShader(vs);
    if (fs) glDeleteShader(fs);
    return -1;
  }
  gl_program_nv12 = glCreateProgram();
  glAttachShader(gl_program_nv12, vs);
  glAttachShader(gl_program_nv12, fs);
  glBindAttribLocation(gl_program_nv12, 0, "a_pos");
  glBindAttribLocation(gl_program_nv12, 1, "a_uv");
  glLinkProgram(gl_program_nv12);
  glDeleteShader(vs);
  glDeleteShader(fs);
  GLint linked = 0;
  glGetProgramiv(gl_program_nv12, GL_LINK_STATUS, &linked);
  if (!linked) {
    char log[1024] = {0};
    glGetProgramInfoLog(gl_program_nv12, sizeof log - 1, NULL, log);
    LOG("gl_init_nv12: program link failed: %s", log);
    return -1;
  }
  gl_uloc_y_nv12  = glGetUniformLocation(gl_program_nv12, "u_tex_y");
  gl_uloc_uv_nv12 = glGetUniformLocation(gl_program_nv12, "u_tex_uv");
  gl_uloc_range_nv12 = glGetUniformLocation(gl_program_nv12, "u_range");
  gl_uloc_coef_nv12 = glGetUniformLocation(gl_program_nv12, "u_coef");
  gl_nv12_colorspace = -1;
  gl_nv12_full_range = -1;
  glUseProgram(gl_program_nv12);
  set_nv12_color(COLORSPACE_REC_709, false);

  /* Y plane texture: single-channel 8-bit, full resolution. */
  glGenTextures(1, &gl_texture_y);
  glBindTexture(GL_TEXTURE_2D, gl_texture_y);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
  glTexImage2D(GL_TEXTURE_2D, 0, GL_R8, width, height, 0,
               GL_RED, GL_UNSIGNED_BYTE, NULL);

  /* UV plane texture: two-channel 8-bit, half resolution in both axes
   * (NV12 chroma is 2x2-subsampled). */
  glGenTextures(1, &gl_texture_uv);
  glBindTexture(GL_TEXTURE_2D, gl_texture_uv);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
  glTexImage2D(GL_TEXTURE_2D, 0, GL_RG8, width / 2, height / 2, 0,
               GL_RG, GL_UNSIGNED_BYTE, NULL);

  gl_nv12_width  = width;
  gl_nv12_height = height;
  gl_nv12_inited = true;
  LOG("gl_init_nv12: program=%u y=%u uv=%u %dx%d",
      gl_program_nv12, gl_texture_y, gl_texture_uv, width, height);
  return 0;
}

/* ------------------------------------------------------------------ */
/* Per-frame import + draw                                            */
/* ------------------------------------------------------------------ */

static void direct_update_dmabuf_quad_if_needed(AVFrame *frame, int import_h) {
  if (direct_quad_view_w == gl_viewport_w &&
      direct_quad_view_h == gl_viewport_h &&
      direct_quad_dst_x == sdl_dst_rect.x &&
      direct_quad_dst_y == sdl_dst_rect.y &&
      direct_quad_dst_w == sdl_dst_rect.w &&
      direct_quad_dst_h == sdl_dst_rect.h &&
      direct_quad_import_h == import_h &&
      direct_quad_visible_h == frame->height)
    return;

  float uv_vmax = import_h > 0 ? (float) frame->height / (float) import_h : 1.0f;
  if (uv_vmax > 1.0f) uv_vmax = 1.0f;
  float left   = ((float) sdl_dst_rect.x / (float) gl_viewport_w) * 2.0f - 1.0f;
  float right  = ((float) (sdl_dst_rect.x + sdl_dst_rect.w) / (float) gl_viewport_w) * 2.0f - 1.0f;
  float top    = 1.0f - ((float) sdl_dst_rect.y / (float) gl_viewport_h) * 2.0f;
  float bottom = 1.0f - ((float) (sdl_dst_rect.y + sdl_dst_rect.h) / (float) gl_viewport_h) * 2.0f;
  GLfloat quad[] = {
    left,  bottom, 0.0f, uv_vmax,
    right, bottom, 1.0f, uv_vmax,
    left,  top,    0.0f, 0.0f,
    right, top,    1.0f, 0.0f,
  };

  glBindBuffer(GL_ARRAY_BUFFER, gl_vbo);
  glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_DYNAMIC_DRAW);

  direct_quad_view_w = gl_viewport_w;
  direct_quad_view_h = gl_viewport_h;
  direct_quad_dst_x = sdl_dst_rect.x;
  direct_quad_dst_y = sdl_dst_rect.y;
  direct_quad_dst_w = sdl_dst_rect.w;
  direct_quad_dst_h = sdl_dst_rect.h;
  direct_quad_import_h = import_h;
  direct_quad_visible_h = frame->height;
}

/* Forward decls for the dispatching draw_frame(). */
static void draw_frame_drm_prime(AVFrame *frame);

static int stream_width, stream_height;
static int create_window(int width, int height);

static int visible_from_hint(int coded, int hint, int max_pad) {
  return (hint > 0 && hint <= coded && coded - hint <= max_pad) ? hint : coded;
}

static int frame_visible_width(const AVFrame *frame) {
  int cropped = frame->width - (int)frame->crop_left - (int)frame->crop_right;
  return cropped > 0 && cropped != frame->width ? cropped :
      visible_from_hint(frame->width, stream_width, 127);
}

static int frame_visible_height(const AVFrame *frame) {
  int cropped = frame->height - (int)frame->crop_top - (int)frame->crop_bottom;
  return cropped > 0 && cropped != frame->height ? cropped :
      visible_from_hint(frame->height, stream_height, 31);
}

static void draw_frame(AVFrame *frame) {
  static int draw_count = 0;
  if (++draw_count <= 3)
    LOG("draw_frame: #%d size=%dx%d fmt=%s linesize=[%d,%d,%d,%d]",
        draw_count, frame->width, frame->height,
        av_get_pix_fmt_name(frame->format),
        frame->linesize[0], frame->linesize[1],
        frame->linesize[2], frame->linesize[3]);
  int visible_w = frame_visible_width(frame), visible_h = frame_visible_height(frame);
  pump_sdl_window_events();
  /* Re-cut of legacy 0013: resize only when visible dimensions change.
   * No runtime/presenter reset machinery from legacy 0011 is needed. */
  static int previous_width, previous_height;
  const char *auto_resize = getenv("MOONLIGHT_AUTO_WINDOW_RESIZE");
  if (window && auto_resize && strcmp(auto_resize, "1") == 0 &&
      (previous_width != visible_w || previous_height != visible_h)) {
    previous_width = visible_w;
    previous_height = visible_h;
    SDL_SetWindowSize(window, previous_width, previous_height);
  }
  update_render_geometry(visible_w, visible_h);
  if (frame->format == AV_PIX_FMT_DRM_PRIME) {
    draw_frame_drm_prime(frame);
  } else {
    static bool warned = false;
    if (!warned) {
      LOG("draw_frame: unsupported pix_fmt=%s -- no DRM_PRIME or NV12 path,"
          " pixel data discarded", av_get_pix_fmt_name(frame->format));
      warned = true;
    }
  }
}

static void prime_cache_clear(void) {
  for (int i = 0; i < prime_cache_count; i++) {
    PrimeCacheEntry *e = &prime_cache[i];
    if (e->tex_y) glDeleteTextures(1, &e->tex_y);
    if (e->tex_uv) glDeleteTextures(1, &e->tex_uv);
    if (e->img_y != EGL_NO_IMAGE_KHR) egl_destroy_image(egl_dpy, e->img_y);
    if (e->img_uv != EGL_NO_IMAGE_KHR) egl_destroy_image(egl_dpy, e->img_uv);
  }
  memset(prime_cache, 0, sizeof prime_cache);
  prime_cache_count = 0;
  prime_cache_frames = NULL;
}

static EGLImageKHR prime_import_plane(int fd, uint32_t fourcc, int width,
                                      int height, int offset, int pitch) {
  EGLint attribs[] = {
    EGL_WIDTH, width,
    EGL_HEIGHT, height,
    EGL_LINUX_DRM_FOURCC_EXT, (EGLint) fourcc,
    EGL_DMA_BUF_PLANE0_FD_EXT, fd,
    EGL_DMA_BUF_PLANE0_OFFSET_EXT, offset,
    EGL_DMA_BUF_PLANE0_PITCH_EXT, pitch,
    EGL_NONE
  };
  EGLImageKHR img = egl_create_image(egl_dpy, EGL_NO_CONTEXT,
                                     EGL_LINUX_DMA_BUF_EXT,
                                     (EGLClientBuffer) NULL, attribs);
  if (img == EGL_NO_IMAGE_KHR)
    LOG("v4l2request: plane import failed err=0x%04x fourcc=0x%08x fd=%d "
        "size=%dx%d offset=%d pitch=%d",
        eglGetError(), fourcc, fd, width, height, offset, pitch);
  return img;
}

static GLuint prime_bind_texture(EGLImageKHR img) {
  GLuint tex = 0;
  glGenTextures(1, &tex);
  glBindTexture(GL_TEXTURE_2D, tex);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
  glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
  gl_image_target_texture(GL_TEXTURE_2D, img);
  return tex;
}

/* Return the cached textures for this capture buffer, importing it on
 * first sight. FFmpeg's V4L2 Request frames pool keeps each capture
 * buffer, and so its dma-buf fd, for the life of the frames context, so
 * the fd identifies the buffer until the context changes. */
static PrimeCacheEntry *prime_cache_lookup(const AVFrame *frame,
                                           const AVDRMFrameDescriptor *desc,
                                           int import_w, int import_h) {
  const void *frames = frame->hw_frames_ctx ? frame->hw_frames_ctx->data : NULL;
  if (frames != prime_cache_frames) {
    if (prime_cache_count)
      LOG("v4l2request: frames context changed, dropping %d cached buffers",
          prime_cache_count);
    prime_cache_clear();
    prime_cache_frames = frames;
  }

  const AVDRMLayerDescriptor *layer = &desc->layers[0];
  int fd = desc->objects[layer->planes[0].object_index].fd;
  int uv_fd = desc->objects[layer->planes[1].object_index].fd;
  int pitch = (int) layer->planes[0].pitch;
  int uv_offset = (int) layer->planes[1].offset;
  for (int i = 0; i < prime_cache_count; i++) {
    PrimeCacheEntry *e = &prime_cache[i];
    if (e->fd == fd && e->width == import_w && e->height == import_h &&
        e->pitch == pitch && e->uv_offset == uv_offset)
      return e;
  }
  if (prime_cache_count == PRIME_CACHE_SIZE) {
    LOG("v4l2request: more than %d capture buffers, rebuilding cache",
        PRIME_CACHE_SIZE);
    prime_cache_clear();
    prime_cache_frames = frames;
  }

  EGLImageKHR img_y = prime_import_plane(fd, DRM_FORMAT_R8, import_w, import_h,
                                         (int) layer->planes[0].offset, pitch);
  EGLImageKHR img_uv = prime_import_plane(uv_fd, DRM_FORMAT_GR88,
                                          import_w / 2, import_h / 2,
                                          uv_offset, (int) layer->planes[1].pitch);
  if (img_y == EGL_NO_IMAGE_KHR || img_uv == EGL_NO_IMAGE_KHR) {
    if (img_y != EGL_NO_IMAGE_KHR) egl_destroy_image(egl_dpy, img_y);
    if (img_uv != EGL_NO_IMAGE_KHR) egl_destroy_image(egl_dpy, img_uv);
    return NULL;
  }
  PrimeCacheEntry *e = &prime_cache[prime_cache_count++];
  e->fd = fd;
  e->width = import_w;
  e->height = import_h;
  e->pitch = pitch;
  e->uv_offset = uv_offset;
  e->img_y = img_y;
  e->img_uv = img_uv;
  e->tex_y = prime_bind_texture(img_y);
  e->tex_uv = prime_bind_texture(img_uv);
  GLenum glerr = glGetError();
  if (glerr != GL_NO_ERROR) {
    LOG("v4l2request: glEGLImageTargetTexture2DOES error=0x%04x fd=%d", glerr, fd);
    prime_cache_count--;
    glDeleteTextures(1, &e->tex_y);
    glDeleteTextures(1, &e->tex_uv);
    egl_destroy_image(egl_dpy, img_y);
    egl_destroy_image(egl_dpy, img_uv);
    memset(e, 0, sizeof *e);
    return NULL;
  }
  if (prime_cache_count <= 2)
    LOG("v4l2request: imported capture buffer fd=%d import=%dx%d pitch=%d uv_offset=%d",
        fd, import_w, import_h, pitch, uv_offset);
  return e;
}

static void draw_frame_drm_prime(AVFrame *frame) {
  static bool warned = false;
  if (!request_mode || !gl_ctx) {
    if (!warned) {
      LOG("draw_frame: DRM_PRIME frame received but SDL renderer path is active; frame discarded");
      warned = true;
    }
    return;
  }
  const AVDRMFrameDescriptor *desc = (const AVDRMFrameDescriptor *) frame->data[0];
  const AVDRMLayerDescriptor *layer = desc ? &desc->layers[0] : NULL;
  uint64_t modifier = desc ? desc->objects[0].format_modifier : 0;
  if (!desc || desc->nb_layers != 1 || layer->format != DRM_FORMAT_NV12 ||
      layer->nb_planes != 2 || layer->planes[0].pitch <= 0 ||
      (modifier != DRM_FORMAT_MOD_LINEAR && modifier != DRM_FORMAT_MOD_INVALID)) {
    if (!warned) {
      LOG("v4l2request: unsupported DRM_PRIME frame layers=%d fourcc=0x%08x planes=%d "
          "modifier=0x%" PRIx64 "; only linear NV12 is presented",
          desc ? desc->nb_layers : -1, layer ? layer->format : 0,
          layer ? layer->nb_planes : -1, modifier);
      warned = true;
    }
    return;
  }

  /* The capture buffer holds the coded size and the UV plane starts
   * after the coded rows. FFmpeg crops hwaccel frames by reducing
   * width and height, so importing exactly the frame's size shows only
   * the visible picture. */
  int import_w = frame->width & ~1;
  int import_h = frame->height & ~1;
  if (import_w <= 0 || import_h <= 0 || import_w > (int) layer->planes[0].pitch)
    return;

  if (gl_init_nv12(import_w, import_h) != 0)
    return;
  PrimeCacheEntry *e = prime_cache_lookup(frame, desc, import_w, import_h);
  if (!e)
    return;

  direct_update_dmabuf_quad_if_needed(frame, import_h);

  glViewport(0, 0, gl_viewport_w, gl_viewport_h);
  glClear(GL_COLOR_BUFFER_BIT);
  glUseProgram(gl_program_nv12);
  /* Sunshine's hardware encoders often omit the VUI, so the frame's
   * colorspace is usually unspecified: use the colorspace the host
   * reports with each decode unit, and the frame's range when the
   * stream states it (Moonlight requests limited range). */
  set_nv12_color(frame->colorspace == AVCOL_SPC_BT709 ? COLORSPACE_REC_709 :
                 frame->colorspace == AVCOL_SPC_SMPTE170M ||
                 frame->colorspace == AVCOL_SPC_BT470BG ? COLORSPACE_REC_601 :
                 stream_colorspace,
                 frame->color_range == AVCOL_RANGE_JPEG);
  glActiveTexture(GL_TEXTURE0);
  glBindTexture(GL_TEXTURE_2D, e->tex_y);
  glUniform1i(gl_uloc_y_nv12, 0);
  glActiveTexture(GL_TEXTURE1);
  glBindTexture(GL_TEXTURE_2D, e->tex_uv);
  glUniform1i(gl_uloc_uv_nv12, 1);
  glBindBuffer(GL_ARRAY_BUFFER, gl_vbo);
  glEnableVertexAttribArray(0);
  glEnableVertexAttribArray(1);
  glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(GLfloat), (void *)0);
  glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(GLfloat), (void *)(2 * sizeof(GLfloat)));
  glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
  GLenum glerr = glGetError();
  if (glerr != GL_NO_ERROR) {
    static bool logged_draw_error = false;
    if (!logged_draw_error) {
      LOG("v4l2request: glDrawArrays error=0x%04x", glerr);
      logged_draw_error = true;
    }
  }
  glDisableVertexAttribArray(0);
  glDisableVertexAttribArray(1);
  SDL_GL_SwapWindow(window);
}
/* ------------------------------------------------------------------ */
/* Display thread                                                     */
/* ------------------------------------------------------------------ */

static void *display_loop(void *arg) {
  (void) arg;

  if (request_mode && create_window(stream_width, stream_height) != 0) {
    LOG("v4l2request: window creation failed; no frames will be shown");
    pthread_mutex_lock(&disp_mtx);
    disp_setup_rc = -1;
    disp_ready = true;
    pthread_cond_signal(&disp_cond);
    pthread_mutex_unlock(&disp_mtx);
    return NULL;
  }


  int out_w = 0, out_h = 0;
  if (direct_dmabuf_present) {
    gl_ctx = SDL_GL_CreateContext(window);
    if (gl_ctx)
      SDL_GL_MakeCurrent(window, gl_ctx);
    if (!gl_ctx) {
      LOG("SDL_GL_CreateContext failed: %s", SDL_GetError());
      pthread_mutex_lock(&disp_mtx);
      disp_setup_rc = -1;
      disp_ready = true;
      pthread_cond_signal(&disp_cond);
      pthread_mutex_unlock(&disp_mtx);
      return NULL;
    }
    SDL_GL_SetSwapInterval(1);
    egl_dpy = eglGetCurrentDisplay();
    SDL_GL_GetDrawableSize(window, &out_w, &out_h);
    if (out_w <= 0 || out_h <= 0)
      SDL_GetWindowSize(window, &out_w, &out_h);
    if (gl_init(out_w, out_h) != 0) {
      LOG("direct-dmabuf: GL/EGL init failed");
      pthread_mutex_lock(&disp_mtx);
      disp_setup_rc = -1;
      disp_ready = true;
      pthread_cond_signal(&disp_cond);
      pthread_mutex_unlock(&disp_mtx);
      return NULL;
    }
    LOG("display thread: direct-dmabuf GL output=%dx%d display=%d driver=%s egl=%p",
        out_w, out_h, SDL_GetWindowDisplayIndex(window),
        SDL_GetCurrentVideoDriver() ? SDL_GetCurrentVideoDriver() : "unknown",
        (void *) egl_dpy);
  }

  pthread_mutex_lock(&disp_mtx);
  disp_setup_rc = 0;
  disp_ready = true;
  pthread_cond_signal(&disp_cond);
  pthread_mutex_unlock(&disp_mtx);

  /* Request mode samples the decoder's capture buffer directly. Keep
   * the last presented frame referenced until the next one is drawn so
   * the decoder cannot reuse that buffer while the GPU still reads it. */
  AVFrame *shown = NULL;
  for (;;) {
    AVFrame *frame = NULL;

    pthread_mutex_lock(&disp_mtx);
    while (!disp_pending && !disp_eos)
      pthread_cond_wait(&disp_cond, &disp_mtx);
    if (disp_eos) {
      if (disp_pending) {
        av_frame_free(&disp_pending);
        disp_pending = NULL;
      }
      pthread_mutex_unlock(&disp_mtx);
      break;
    }
    frame = disp_pending;
    disp_pending = NULL;
    pthread_mutex_unlock(&disp_mtx);

    draw_frame(frame);
    if (request_mode) {
      av_frame_free(&shown);
      shown = frame;
    } else {
      av_frame_free(&frame);
    }
  }
  av_frame_free(&shown);
  if (request_mode && egl_destroy_image)
    prime_cache_clear();
  return NULL;
}

/* ------------------------------------------------------------------ */
/* Moonlight callbacks                                                */
/* ------------------------------------------------------------------ */

static int create_window(int width, int height) {
  /* SDL2 GL window. The stream size and the compositor-visible window
   * size are separate concerns. Create an initially-hidden, resizable
   * GL surface, choose an initial display size from SDL's display bounds,
   * then let runtime window/display events keep the viewport correct.
   *
   * MOONLIGHT_DISPLAY_INDEX can force a display for dual-screen devices
   * like AYN Thor. Without it, SDL's default display selection is used
   * and subsequent compositor moves/resizes are handled dynamically. */
  if (SDL_Init(SDL_INIT_VIDEO) < 0) {
    LOG("SDL_Init: %s", SDL_GetError());
    return -1;
  }
  int display_index = 0;
  const char *display_env = getenv("MOONLIGHT_DISPLAY_INDEX");
  if (display_env && *display_env) {
    char *endptr = NULL;
    long parsed = strtol(display_env, &endptr, 10);
    if (endptr && *endptr == '\0' && parsed >= 0 && parsed < SDL_GetNumVideoDisplays())
      display_index = (int) parsed;
    else
      LOG("setup: ignoring invalid MOONLIGHT_DISPLAY_INDEX=%s", display_env);
  }

  SDL_Rect bounds = {0, 0, width, height};
  if (SDL_GetDisplayUsableBounds(display_index, &bounds) != 0 ||
      bounds.w <= 0 || bounds.h <= 0) {
    LOG("SDL_GetDisplayUsableBounds(%d) failed, falling back to stream size: %s",
        display_index, SDL_GetError());
    bounds.x = 0;
    bounds.y = 0;
    bounds.w = width;
    bounds.h = height;
  }
  LOG("setup: stream=%dx%d display[%d]_bounds=%dx%d@%d,%d displays=%d",
      width, height, display_index, bounds.w, bounds.h, bounds.x, bounds.y,
      SDL_GetNumVideoDisplays());

  window = SDL_CreateWindow("Moonlight",
                            bounds.x, bounds.y,
                            bounds.w, bounds.h,
                            SDL_WINDOW_BORDERLESS |
                            SDL_WINDOW_RESIZABLE | SDL_WINDOW_HIDDEN |
                            (direct_dmabuf_present ? SDL_WINDOW_OPENGL : 0));
  if (window) {
    /* Ensure SDL's surface size starts at the selected display bounds even
     * if the compositor adjusted placement during creation. */
    SDL_SetWindowPosition(window, bounds.x, bounds.y);
    SDL_SetWindowSize(window, bounds.w, bounds.h);
    SDL_ShowWindow(window);
    /* Request mode targets single-screen handhelds whose tiling
     * compositor would otherwise split the screen with the portal. */
    if (request_mode &&
        SDL_SetWindowFullscreen(window, SDL_WINDOW_FULLSCREEN_DESKTOP) != 0)
      LOG("v4l2request: fullscreen request failed: %s", SDL_GetError());
  }
  if (!window) {
    LOG("SDL_CreateWindow: %s", SDL_GetError());
    return -1;
  }
  return 0;
}

static int v4l2m2m_setup(int videoFormat, int width, int height,
                         int redrawRate, void *context, int drFlags) {
  stream_width = width;
  stream_height = height;
  (void) redrawRate; (void) context; (void) drFlags;

  if (request_mode) {
    /* Request-mode frames are dma-bufs: present them through the GLES
     * context, never through the SDL renderer. */
    direct_dmabuf_present = true;
    /* dma-buf import needs an EGL display. Under X11 (Xwayland) SDL
     * would otherwise create the GLES context through GLX. */
    SDL_SetHint(SDL_HINT_VIDEO_X11_FORCE_EGL, "1");
  }
  if (direct_dmabuf_present) {
    LOG("setup: using experimental direct V4L2 dma-buf GL presentation path");
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_ES);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 3);
    SDL_GL_SetAttribute(SDL_GL_CONTEXT_MINOR_VERSION, 0);
    SDL_GL_SetAttribute(SDL_GL_DOUBLEBUFFER, 1);
  }
  if (request_mode) {
    if (request_decoder_setup(videoFormat, width, height) != 0)
      return -1;
  }

  disp_eos    = false;
  disp_ready  = false;
  disp_setup_rc = 0;
  if (request_mode) {
    /* EGL bring-up on the H700 takes several seconds, longer than the
     * host waits for the client's first ping while setup() blocks the
     * video stream start. Create the window and GLES context on the
     * display thread instead and return at once; frames that arrive
     * before it is ready are replaced by newer ones and never shown. */
    pthread_create(&disp_thread, NULL, display_loop, NULL);
    LOG("setup: decoder=v4l2request presenter=prime-gl %dx%d (display starts in background)",
        width, height);
    return 0;
  }
  return 0;
}

static void v4l2m2m_cleanup(void) {
  /* Signal display thread to exit. It may have a pending frame in
   * flight; the wakeup branch in display_loop frees it. */
  pthread_mutex_lock(&disp_mtx);
  disp_eos = true;
  pthread_cond_signal(&disp_cond);
  pthread_mutex_unlock(&disp_mtx);
  pthread_join(disp_thread, NULL);
  /* The display thread may have exited early (request mode window or
   * GL failure) and left a submitted frame behind. */
  if (disp_pending) av_frame_free(&disp_pending);

  if (sdl_renderer)     { SDL_DestroyRenderer(sdl_renderer); sdl_renderer = NULL; }
  if (gl_program)      { glDeleteProgram(gl_program); gl_program = 0; }
  if (gl_texture)      { glDeleteTextures(1, &gl_texture); gl_texture = 0; }
  if (gl_program_nv12) { glDeleteProgram(gl_program_nv12); gl_program_nv12 = 0; }
  if (gl_texture_y)    { glDeleteTextures(1, &gl_texture_y); gl_texture_y = 0; }
  if (gl_texture_uv)   { glDeleteTextures(1, &gl_texture_uv); gl_texture_uv = 0; }
  gl_nv12_inited = false;
  gl_viewport_w = gl_viewport_h = gl_video_w = gl_video_h = 0;
  direct_quad_view_w = direct_quad_view_h = direct_quad_dst_x = direct_quad_dst_y = 0;
  direct_quad_dst_w = direct_quad_dst_h = direct_quad_import_h = direct_quad_visible_h = 0;
  if (gl_vbo)          { glDeleteBuffers(1, &gl_vbo); gl_vbo = 0; }

  if (gl_ctx)  { SDL_GL_DeleteContext(gl_ctx); gl_ctx = NULL; }
  if (window)  { SDL_DestroyWindow(window); window = NULL; }
  SDL_QuitSubSystem(SDL_INIT_VIDEO);

  direct_dmabuf_present = false;
  if (pkt)        av_packet_free(&pkt);
  if (codec_ctx)  avcodec_free_context(&codec_ctx);
  if (nal_buf)    { free(nal_buf); nal_buf = NULL; nal_size = 0; }
}

static int v4l2m2m_submit(PDECODE_UNIT du) {
  stream_colorspace = du->colorspace;

  /* Coalesce the linked-list of bufferList entries into a single
   * contiguous packet, plus the AV_INPUT_BUFFER_PADDING_SIZE FFmpeg
   * requires after the payload. Same shape PR #932's ffmpeg_drm uses. */
  ensure_buf_size(&nal_buf, &nal_size,
                  du->fullLength + AV_INPUT_BUFFER_PADDING_SIZE);
  memset((uint8_t *) nal_buf + du->fullLength, 0, AV_INPUT_BUFFER_PADDING_SIZE);
  int len = 0;
  for (PLENTRY e = du->bufferList; e; e = e->next) {
    memcpy((uint8_t *) nal_buf + len, e->data, e->length);
    len += e->length;
  }
  pkt->data = (uint8_t *) nal_buf;
  pkt->size = len;

  if (avcodec_send_packet(codec_ctx, pkt) < 0)
    return DR_NEED_IDR;

  AVFrame *f = av_frame_alloc();
  if (!f)
    return DR_OK;

  int ret;
  while ((ret = avcodec_receive_frame(codec_ctx, f)) == 0) {
    static bool logged_first_frame = false;
    if (!logged_first_frame) {
      LOG("submit: first frame from decoder format=%s (DRM_PRIME=%s)",
          av_get_pix_fmt_name(f->format),
          f->format == AV_PIX_FMT_DRM_PRIME ? "YES" : "no");
      logged_first_frame = true;
    }
    /* Accept both DRM_PRIME (zero-copy path, currently dead -- see
     * note above gl_program_nv12) and NV12 (Plan A path -- what the
     * FFmpeg v4l2m2m wrapper actually emits on the iris driver). The
     * format branch in draw_frame() picks the right upload path.
     *
     * av_frame_clone is required for BOTH paths: for DRM_PRIME it
     * keeps the dmabuf object refcounted; for NV12 it keeps the V4L2
     * MMAP'd plane data alive until the display thread finishes the
     * glTexSubImage2D upload (otherwise FFmpeg could recycle the V4L2
     * buffer while the GL driver is still reading it). */
    if (f->format == AV_PIX_FMT_DRM_PRIME || f->format == AV_PIX_FMT_NV12) {
      AVFrame *clone = av_frame_clone(f);
      if (clone) {
        pthread_mutex_lock(&disp_mtx);
        if (disp_pending) av_frame_free(&disp_pending);  /* drop if display lagging */
        disp_pending = clone;
        pthread_cond_signal(&disp_cond);
        pthread_mutex_unlock(&disp_mtx);
      }
    }
    av_frame_unref(f);
  }
  av_frame_free(&f);

  return (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) ? DR_OK : DR_NEED_IDR;
}

static int v4l2request_setup(int videoFormat, int width, int height,
                             int redrawRate, void *context, int drFlags) {
  request_mode = true;
  return v4l2m2m_setup(videoFormat, width, height, redrawRate, context, drFlags);
}

DECODER_RENDERER_CALLBACKS decoder_callbacks_v4l2request = {
  .setup            = v4l2request_setup,
  .cleanup          = v4l2m2m_cleanup,
  .submitDecodeUnit = v4l2m2m_submit,
  .capabilities     = CAPABILITY_DIRECT_SUBMIT,
};
