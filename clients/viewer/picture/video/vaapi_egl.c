// SPDX-License-Identifier: GPL-3.0-or-later
#include "vaapi_egl.h"

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <GLES2/gl2ext.h>

#include <stdio.h>
#include <string.h>

// DRM_FORMAT_MOD_INVALID, without a libdrm dependency on the PC build.
#define MODIFIER_INVALID 0x00ffffffffffffffULL
#define MAX_PLANES 3

static const char* vertex_source =
  "attribute vec2 position;"
  "uniform vec2 visible;"
  "varying mediump vec2 at;"
  "void main() {"
  "  gl_Position = vec4(position, 0, 1);"
  "  at = vec2((position.x + 1.) / 2., (1. - position.y) / 2.) * visible;"
  "}";
// The driver converts to RGB, by the colour hints of the image.
static const char* fragment_source =
  "#extension GL_OES_EGL_image_external : require\n"
  "uniform samplerExternalOES picture;"
  "uniform mediump vec2 last;"
  "varying mediump vec2 at;"
  "void main() { gl_FragColor = texture2D(picture, min(at, last)); }";
static const float corners[] = { -1, 1, -1, -1, 1, 1, 1, -1 };

static EGLDisplay display = EGL_NO_DISPLAY;
static EGLSurface surface = EGL_NO_SURFACE;
static EGLContext context = EGL_NO_CONTEXT;
static PFNEGLCREATEIMAGEKHRPROC create_image;
static PFNEGLDESTROYIMAGEKHRPROC destroy_image;
static PFNGLEGLIMAGETARGETTEXTURE2DOESPROC image_target_texture;
static bool modifiers;
static GLint visible_uniform, last_uniform;
static bool ready;

static bool fail(const char* what) {
  fprintf(stderr, "VAAPI EGL presentation: %s (EGL 0x%x)\n", what, eglGetError());
  return false;
}

static GLuint compile(GLenum type, const char* source) {
  GLuint shader = glCreateShader(type);
  glShaderSource(shader, 1, &source, NULL);
  glCompileShader(shader);
  return shader;
}

static bool setup(Display* x11, Window window) {
  display = eglGetDisplay((EGLNativeDisplayType)x11);
  EGLint major, minor;
  if (display == EGL_NO_DISPLAY || !eglInitialize(display, &major, &minor))
    return fail("no EGL display");
  const char* extensions = eglQueryString(display, EGL_EXTENSIONS);
  if (!extensions || !strstr(extensions, "EGL_EXT_image_dma_buf_import"))
    return fail("EGL cannot import a dmabuf");
  modifiers = strstr(extensions, "EGL_EXT_image_dma_buf_import_modifiers") != NULL;

  static const EGLint config_attributes[] = {
    EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8,
    EGL_SURFACE_TYPE, EGL_WINDOW_BIT, EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT, EGL_NONE };
  static const EGLint context_attributes[] = { EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE };
  EGLConfig config;
  EGLint found = 0;
  if (!eglChooseConfig(display, config_attributes, &config, 1, &found) || found == 0)
    return fail("no window config");
  if (!eglBindAPI(EGL_OPENGL_ES_API))
    return fail("no OpenGL ES");
  context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attributes);
  surface = eglCreateWindowSurface(display, config, (EGLNativeWindowType)window, NULL);
  if (context == EGL_NO_CONTEXT || surface == EGL_NO_SURFACE ||
      !eglMakeCurrent(display, surface, surface, context))
    return fail("no context on the window");
  // The newest frame goes out at once. The compositor picks the frame that
  // each refresh shows, so a wait for the display here only adds delay.
  eglSwapInterval(display, 0);

  const char* gl_extensions = (const char*)glGetString(GL_EXTENSIONS);
  create_image = (PFNEGLCREATEIMAGEKHRPROC)eglGetProcAddress("eglCreateImageKHR");
  destroy_image = (PFNEGLDESTROYIMAGEKHRPROC)eglGetProcAddress("eglDestroyImageKHR");
  image_target_texture =
    (PFNGLEGLIMAGETARGETTEXTURE2DOESPROC)eglGetProcAddress("glEGLImageTargetTexture2DOES");
  if (!gl_extensions || !strstr(gl_extensions, "GL_OES_EGL_image_external") ||
      !create_image || !destroy_image || !image_target_texture)
    return fail("OpenGL ES cannot sample an external image");

  GLuint program = glCreateProgram();
  glAttachShader(program, compile(GL_VERTEX_SHADER, vertex_source));
  glAttachShader(program, compile(GL_FRAGMENT_SHADER, fragment_source));
  glBindAttribLocation(program, 0, "position");
  glLinkProgram(program);
  GLint linked = GL_FALSE;
  glGetProgramiv(program, GL_LINK_STATUS, &linked);
  if (!linked)
    return fail("the shader did not link");
  glUseProgram(program);
  visible_uniform = glGetUniformLocation(program, "visible");
  last_uniform = glGetUniformLocation(program, "last");
  glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 0, corners);
  glEnableVertexAttribArray(0);

  GLuint texture;
  glGenTextures(1, &texture);
  glBindTexture(GL_TEXTURE_EXTERNAL_OES, texture);
  glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
  glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
  glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
  return true;
}

static EGLImageKHR import(const VADRMPRIMESurfaceDescriptor* picture, const AVFrame* frame) {
  static const EGLint fd[] = {
    EGL_DMA_BUF_PLANE0_FD_EXT, EGL_DMA_BUF_PLANE1_FD_EXT, EGL_DMA_BUF_PLANE2_FD_EXT };
  static const EGLint offset[] = {
    EGL_DMA_BUF_PLANE0_OFFSET_EXT, EGL_DMA_BUF_PLANE1_OFFSET_EXT, EGL_DMA_BUF_PLANE2_OFFSET_EXT };
  static const EGLint pitch[] = {
    EGL_DMA_BUF_PLANE0_PITCH_EXT, EGL_DMA_BUF_PLANE1_PITCH_EXT, EGL_DMA_BUF_PLANE2_PITCH_EXT };
  static const EGLint modifier_low[] = {
    EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, EGL_DMA_BUF_PLANE1_MODIFIER_LO_EXT,
    EGL_DMA_BUF_PLANE2_MODIFIER_LO_EXT };
  static const EGLint modifier_high[] = {
    EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, EGL_DMA_BUF_PLANE1_MODIFIER_HI_EXT,
    EGL_DMA_BUF_PLANE2_MODIFIER_HI_EXT };

  // A composed export: one layer that holds every plane of the picture.
  if (picture->num_layers != 1 || picture->layers[0].num_planes == 0 ||
      picture->layers[0].num_planes > MAX_PLANES)
    return EGL_NO_IMAGE_KHR;
  EGLint colour = frame->colorspace == AVCOL_SPC_BT709 ? EGL_ITU_REC709_EXT :
    frame->colorspace == AVCOL_SPC_BT2020_NCL ? EGL_ITU_REC2020_EXT : EGL_ITU_REC601_EXT;
  EGLint range = frame->color_range == AVCOL_RANGE_JPEG ?
    EGL_YUV_FULL_RANGE_EXT : EGL_YUV_NARROW_RANGE_EXT;
  EGLint attributes[11 + MAX_PLANES * 10] = {
    EGL_WIDTH, picture->width,
    EGL_HEIGHT, picture->height,
    EGL_LINUX_DRM_FOURCC_EXT, picture->layers[0].drm_format,
    EGL_YUV_COLOR_SPACE_HINT_EXT, colour,
    EGL_SAMPLE_RANGE_HINT_EXT, range,
  };
  int next = 10;
  for (uint32_t plane = 0; plane < picture->layers[0].num_planes; ++plane) {
    uint32_t object = picture->layers[0].object_index[plane];
    if (object >= picture->num_objects)
      return EGL_NO_IMAGE_KHR;
    uint64_t modifier = picture->objects[object].drm_format_modifier;
    attributes[next++] = fd[plane];
    attributes[next++] = picture->objects[object].fd;
    attributes[next++] = offset[plane];
    attributes[next++] = picture->layers[0].offset[plane];
    attributes[next++] = pitch[plane];
    attributes[next++] = picture->layers[0].pitch[plane];
    if (modifiers && modifier != MODIFIER_INVALID) {
      attributes[next++] = modifier_low[plane];
      attributes[next++] = (EGLint)(modifier & 0xffffffff);
      attributes[next++] = modifier_high[plane];
      attributes[next++] = (EGLint)(modifier >> 32);
    }
  }
  attributes[next] = EGL_NONE;
  return create_image(display, EGL_NO_CONTEXT, EGL_LINUX_DMA_BUF_EXT, NULL, attributes);
}

bool vaapi_egl_present(Display* x11, Window window, struct picture_place place,
    const VADRMPRIMESurfaceDescriptor* picture, const AVFrame* frame) {
  if (!ready) {
    if (display != EGL_NO_DISPLAY)
      return false; // Setup failed once and said why.
    if (!setup(x11, window))
      return false;
    ready = true;
  }

  EGLImageKHR image = import(picture, frame);
  if (image == EGL_NO_IMAGE_KHR)
    return fail("EGL refused the decoded picture");
  image_target_texture(GL_TEXTURE_EXTERNAL_OES, image);
  // The surface has the coded size. Map the visible part onto its place in
  // the window, and stop at the centre of its last texel, so that the filter
  // does not blend the padding in.
  glUniform2f(visible_uniform,
      (float)frame->width / picture->width, (float)frame->height / picture->height);
  glUniform2f(last_uniform,
      (frame->width - .5f) / picture->width, (frame->height - .5f) / picture->height);
  // Black bars around the picture (#476). A clear covers the whole window,
  // whatever the viewport; OpenGL counts the viewport's rows from the bottom.
  glClearColor(0.f, 0.f, 0.f, 1.f);
  glClear(GL_COLOR_BUFFER_BIT);
  glViewport(place.x, place.window_height - place.y - place.height, place.width, place.height);
  glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
  // The decoder reuses the surface when its frame is released.
  glFinish();
  bool shown = eglSwapBuffers(display, surface);
  destroy_image(display, image);
  return shown || fail("the window did not take the frame");
}

void vaapi_egl_cleanup(void) {
  if (display != EGL_NO_DISPLAY) {
    eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (surface != EGL_NO_SURFACE)
      eglDestroySurface(display, surface);
    if (context != EGL_NO_CONTEXT)
      eglDestroyContext(display, context);
    eglTerminate(display);
  }
  display = EGL_NO_DISPLAY;
  surface = EGL_NO_SURFACE;
  context = EGL_NO_CONTEXT;
  ready = false;
}
