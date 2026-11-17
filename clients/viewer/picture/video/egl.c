/*
 * This file is part of Moonlight Embedded.
 *
 * Copyright (C) 2017 Iwan Timmer
 *
 * Moonlight is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 3 of the License, or
 * (at your option) any later version.
 *
 * Moonlight is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with Moonlight; if not, see <http://www.gnu.org/licenses/>.
 */

#include "egl.h"

#include <Limelight.h>

#include <GLES2/gl2.h>

#include <stdlib.h>
#include <stdio.h>
#include <stdbool.h>
#include <pthread.h>
#include <unistd.h>

static const EGLint context_attributes[] = { EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE };
static const char* texture_mappings[] = { "ymap", "umap", "vmap" };
static const char* vertex_source = "\
attribute vec2 position;\
varying mediump vec2 tex_position;\
\
void main() {\
  gl_Position = vec4(position, 0, 1);\
  tex_position = vec2((position.x + 1.) / 2., (1. - position.y) / 2.);\
}\
";

static const char* fragment_source = "\
uniform lowp sampler2D ymap;\
uniform lowp sampler2D umap;\
uniform lowp sampler2D vmap;\
uniform mediump vec3 visible;\
uniform mediump vec3 last;\
varying mediump vec2 tex_position;\
\
void main() {\
  mediump float y = texture2D(ymap, vec2(min(tex_position.x * visible.x, last.x), tex_position.y)).r;\
  mediump float u = texture2D(umap, vec2(min(tex_position.x * visible.y, last.y), tex_position.y)).r - .5;\n\
  mediump float v = texture2D(vmap, vec2(min(tex_position.x * visible.z, last.z), tex_position.y)).r - .5;\n\
  lowp float r = y + 1.28033 * v;\
  lowp float g = y - .21482 * u - .38059 * v;\
  lowp float b = y + 2.12798 * u;\
  gl_FragColor = vec4(r, g, b, 1.0);\
}\
";

static const float vertices[] = {
  -1.f,  1.f,
  -1.f, -1.f,
  1.f, -1.f,
  1.f, 1.f
};

static const GLuint elements[] = {
  0, 1, 2,
  2, 3, 0
};

static EGLDisplay display;
static EGLSurface surface;
static EGLContext context;

static int width, height;
static bool current;

static GLuint texture_id[3], texture_uniform[3];
static GLuint shader_program;
// Korri (#476): each texture holds whole rows of its plane, padding included,
// and the shader reads only the visible part of them. A new frame size or
// row length makes the textures again.
static int line_sizes[3];
static GLint visible_uniform, last_uniform;

void egl_init(EGLNativeDisplayType native_display, NativeWindowType native_window, int display_width, int display_height) {
  width = display_width;
  height = display_height;

  // get an EGL display connection
  display = eglGetDisplay(native_display);
  if (display == EGL_NO_DISPLAY) {
    fprintf( stderr, "EGL: error get display\n" );
    exit(EXIT_FAILURE);
  }

  // initialize the EGL display connection
  int major, minor;
  EGLBoolean result = eglInitialize(display, &major, &minor);
  if (result == EGL_FALSE) {
    fprintf( stderr, "EGL: error initialising display\n");
    exit(EXIT_FAILURE);
  }

  // get our config from the config class
  EGLConfig config = NULL;
  static const EGLint attribute_list[] = { EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8, EGL_SURFACE_TYPE, EGL_WINDOW_BIT, EGL_NONE };

  EGLint totalConfigsFound = 0;
  result = eglChooseConfig(display, attribute_list, &config, 1, &totalConfigsFound);
  if (result != EGL_TRUE || totalConfigsFound == 0) {
    fprintf(stderr, "EGL: Unable to query for available configs, found %d.\n", totalConfigsFound);
    exit(EXIT_FAILURE);
  }

  // bind the OpenGL API to the EGL
  result = eglBindAPI(EGL_OPENGL_ES_API);
  if (result == EGL_FALSE) {
    fprintf(stderr, "EGL: error binding API\n");
    exit(EXIT_FAILURE);
  }

  // create an EGL rendering context
  context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attributes);
  if (context == EGL_NO_CONTEXT) {
    fprintf(stderr, "EGL: couldn't get a valid context\n");
    exit(EXIT_FAILURE);
  }

  // finally we can create a new surface using this config and window
  surface = eglCreateWindowSurface(display, config, (NativeWindowType) native_window, NULL);
  eglMakeCurrent(display, surface, surface, context);

  glEnable(GL_TEXTURE_2D);

  GLuint vbo;
  glGenBuffers(1, &vbo);
  glBindBuffer(GL_ARRAY_BUFFER, vbo);
  glBufferData(GL_ARRAY_BUFFER, sizeof(vertices), vertices, GL_STATIC_DRAW);

  GLuint ebo;
  glGenBuffers(1, &ebo);
  glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ebo);
  glBufferData(GL_ELEMENT_ARRAY_BUFFER, sizeof(elements), elements, GL_STATIC_DRAW);

  GLuint vertex_shader = glCreateShader(GL_VERTEX_SHADER);
  glShaderSource(vertex_shader, 1, &vertex_source, NULL);
  glCompileShader(vertex_shader);

  GLuint fragment_shader = glCreateShader(GL_FRAGMENT_SHADER);
  glShaderSource(fragment_shader, 1, &fragment_source, NULL);
  glCompileShader(fragment_shader);

  shader_program = glCreateProgram();
  glAttachShader(shader_program, vertex_shader);
  glAttachShader(shader_program, fragment_shader);

  glLinkProgram(shader_program);
  glBindAttribLocation(shader_program, 0, "position");
  glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 2 * sizeof(GLfloat), 0);

  glGenTextures(3, texture_id);
  for (int i = 0; i < 3; i++) {
    glBindTexture(GL_TEXTURE_2D, texture_id[i]);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    // A row's length is seldom a power of two; OpenGL ES 2 samples such a
    // texture only when it clamps.
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_LUMINANCE, i > 0 ? width / 2 : width, i > 0 ? height / 2 : height, 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, 0);

    texture_uniform[i] = glGetUniformLocation(shader_program, texture_mappings[i]);
    line_sizes[i] = 0;
  }
  visible_uniform = glGetUniformLocation(shader_program, "visible");
  last_uniform = glGetUniformLocation(shader_program, "last");

  eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
}

void egl_draw(const AVFrame* frame, struct picture_place place) {
  if (!current) {
    eglMakeCurrent(display, surface, surface, context);
    current = true;
  }

  glUseProgram(shader_program);
  glEnableVertexAttribArray(0);

  int widths[3] = { frame->width, (frame->width + 1) / 2, (frame->width + 1) / 2 };
  int heights[3] = { frame->height, (frame->height + 1) / 2, (frame->height + 1) / 2 };
  bool resized = frame->width != width || frame->height != height;
  GLfloat visible[3], last[3];
  for (int i = 0; i < 3; i++) {
    if (frame->linesize[i] < widths[i])
      return;
    glActiveTexture(GL_TEXTURE0 + i);
    glBindTexture(GL_TEXTURE_2D, texture_id[i]);
    if (resized || frame->linesize[i] != line_sizes[i]) {
      glTexImage2D(GL_TEXTURE_2D, 0, GL_LUMINANCE, frame->linesize[i], heights[i], 0, GL_LUMINANCE, GL_UNSIGNED_BYTE, frame->data[i]);
      line_sizes[i] = frame->linesize[i];
    } else
      glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, frame->linesize[i], heights[i], GL_LUMINANCE, GL_UNSIGNED_BYTE, frame->data[i]);
    glUniform1i(texture_uniform[i], i);
    // Stop at the centre of the last visible texel, so that the filter does
    // not blend the row's padding in.
    visible[i] = (GLfloat)widths[i] / frame->linesize[i];
    last[i] = (widths[i] - .5f) / frame->linesize[i];
  }
  width = frame->width;
  height = frame->height;
  glUniform3f(visible_uniform, visible[0], visible[1], visible[2]);
  glUniform3f(last_uniform, last[0], last[1], last[2]);

  // Black bars around the picture; OpenGL counts rows from the bottom.
  glClearColor(0.f, 0.f, 0.f, 1.f);
  glClear(GL_COLOR_BUFFER_BIT);
  glViewport(place.x, place.window_height - place.y - place.height, place.width, place.height);
  glDrawElements(GL_TRIANGLES, 6, GL_UNSIGNED_INT, 0);

  if (eglSwapBuffers(display, surface))
    picture_shown(frame->width, frame->height);
}

void egl_destroy() {
  eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
  eglDestroySurface(display, surface);
  eglDestroyContext(display, context);
  eglTerminate(display);
}
