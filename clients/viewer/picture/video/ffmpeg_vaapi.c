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

#include <va/va.h>
#include <va/va_x11.h>
#include <va/va_drmcommon.h>
#include <unistd.h>
#include <stdlib.h>
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vaapi.h>
#include <X11/Xlib.h>

#define MAX_SURFACES 16

static AVBufferRef* device_ref;
static AVBufferRef* presentation_ref;
static Display* presentation_x11;
static AVBufferRef* presentation_frames;
static struct imported_surface {
  VASurfaceID decoded, surface;
  struct imported_surface* next;
} *imported_surfaces;

static VADisplay device_display(AVBufferRef* ref) {
  AVHWDeviceContext* device = (AVHWDeviceContext*)ref->data;
  return ((AVVAAPIDeviceContext*)device->hwctx)->display;
}

static void clear_imports(void) {
  while (imported_surfaces) {
    struct imported_surface* entry = imported_surfaces;
    imported_surfaces = entry->next;
    vaDestroySurfaces(device_display(presentation_ref), &entry->surface, 1);
    free(entry);
  }
  av_buffer_unref(&presentation_frames);
}

static enum AVPixelFormat va_get_format(AVCodecContext* context, const enum AVPixelFormat* pixel_format) {
  AVBufferRef* hw_ctx = av_hwframe_ctx_alloc(device_ref);
  if (hw_ctx == NULL) {
    fprintf(stderr, "Failed to initialize Vaapi buffer\n");
    return AV_PIX_FMT_NONE;
  }

  AVHWFramesContext* fr_ctx = (AVHWFramesContext*) hw_ctx->data;
  fr_ctx->format = AV_PIX_FMT_VAAPI;
  fr_ctx->sw_format = AV_PIX_FMT_NV12;
  fr_ctx->width = context->coded_width;
  fr_ctx->height = context->coded_height;
  fr_ctx->initial_pool_size = MAX_SURFACES + 1;

  if (av_hwframe_ctx_init(hw_ctx) < 0) {
    fprintf(stderr, "Failed to initialize VAAPI frame context");
    av_buffer_unref(&hw_ctx);
    return AV_PIX_FMT_NONE;
  }

  context->pix_fmt = AV_PIX_FMT_VAAPI;
  context->hw_frames_ctx = hw_ctx;
  context->slice_flags = SLICE_FLAG_CODED_ORDER | SLICE_FLAG_ALLOW_FIELD;
  return AV_PIX_FMT_VAAPI;
}

static int va_get_buffer(AVCodecContext* context, AVFrame* frame, int flags) {
  return av_hwframe_get_buffer(context->hw_frames_ctx, frame, 0);
}

static void vaapi_device_free(AVHWDeviceContext* device) {
  AVVAAPIDeviceContext* va_ctx = device->hwctx;
  if (va_ctx->display)
    vaTerminate(va_ctx->display);
}

void vaapi_cleanup_lib(void) {
  clear_imports();
  av_buffer_unref(&presentation_ref);
  if (presentation_x11) {
    XCloseDisplay(presentation_x11);
    presentation_x11 = NULL;
  }
  av_buffer_unref(&device_ref);
}

static int create_device(Display* display, AVBufferRef** ref) {
  *ref = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_VAAPI);
  if (!*ref)
    return AVERROR(ENOMEM);

  AVHWDeviceContext* device = (AVHWDeviceContext*)(*ref)->data;
  AVVAAPIDeviceContext* va_ctx = device->hwctx;
  device->free = vaapi_device_free;
  // FFmpeg's device-create helper is built without Xlib in nixpkgs.
  va_ctx->display = vaGetDisplay(display);
  if (!va_ctx->display) {
    av_buffer_unref(ref);
    return AVERROR(ENODEV);
  }

  int major, minor;
  VAStatus status = vaInitialize(va_ctx->display, &major, &minor);
  if (status != VA_STATUS_SUCCESS) {
    fprintf(stderr, "Failed to initialize X11 VAAPI display: %s\n", vaErrorStr(status));
    av_buffer_unref(ref);
    return AVERROR(EIO);
  }

  int result = av_hwdevice_ctx_init(*ref);
  if (result < 0)
    av_buffer_unref(ref);
  return result;
}

int vaapi_init_lib(Display* display) {
  int result = create_device(display, &device_ref);
  if (result < 0)
    return result;

  // vaPutSurface can wait for vblank while holding the VA driver's mutex.
  // An independent VA display keeps that wait out of the decode/UDP thread.
  presentation_x11 = XOpenDisplay(DisplayString(display));
  if (!presentation_x11) {
    vaapi_cleanup_lib();
    return AVERROR(ENODEV);
  }
  result = create_device(presentation_x11, &presentation_ref);
  if (result < 0)
    vaapi_cleanup_lib();
  return result;
}

int vaapi_init(AVCodecContext* decoder_ctx) {
  decoder_ctx->hw_device_ctx = av_buffer_ref(device_ref);
  if (!decoder_ctx->hw_device_ctx)
    return AVERROR(ENOMEM);
  decoder_ctx->get_format = va_get_format;
  decoder_ctx->get_buffer2 = va_get_buffer;
  return 0;
}

static VAStatus import_surface(VASurfaceID decoded, VASurfaceID* surface) {
  for (struct imported_surface* entry = imported_surfaces; entry; entry = entry->next) {
    if (entry->decoded == decoded) {
      *surface = entry->surface;
      return VA_STATUS_SUCCESS;
    }
  }
  VADRMPRIMESurfaceDescriptor descriptor;
  VAStatus status = vaExportSurfaceHandle(device_display(device_ref), decoded,
      VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
      VA_EXPORT_SURFACE_READ_ONLY | VA_EXPORT_SURFACE_COMPOSED_LAYERS, &descriptor);
  if (status != VA_STATUS_SUCCESS)
    return status;

  VASurfaceAttrib attributes[] = {
    {.type = VASurfaceAttribMemoryType, .flags = VA_SURFACE_ATTRIB_SETTABLE,
     .value = {.type = VAGenericValueTypeInteger,
               .value.i = VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2}},
    {.type = VASurfaceAttribExternalBufferDescriptor, .flags = VA_SURFACE_ATTRIB_SETTABLE,
     .value = {.type = VAGenericValueTypePointer, .value.p = &descriptor}},
  };
  status = vaCreateSurfaces(device_display(presentation_ref), VA_RT_FORMAT_YUV420,
      descriptor.width, descriptor.height, surface, 1, attributes, 2);
  for (uint32_t i = 0; i < descriptor.num_objects; ++i)
    close(descriptor.objects[i].fd);
  if (status != VA_STATUS_SUCCESS)
    return status;

  struct imported_surface* entry = malloc(sizeof(*entry));
  if (!entry) {
    vaDestroySurfaces(device_display(presentation_ref), surface, 1);
    return VA_STATUS_ERROR_ALLOCATION_FAILED;
  }
  *entry = (struct imported_surface){decoded, *surface, imported_surfaces};
  imported_surfaces = entry;
  return VA_STATUS_SUCCESS;
}

void vaapi_queue(AVFrame* dec_frame, Window win, int width, int height) {
  // The frame pool owns surface IDs. Retain that pool while imports exist,
  // and discard its imports when the decoder changes pools (e.g. a resize).
  if (!presentation_frames || presentation_frames->data != dec_frame->hw_frames_ctx->data) {
    clear_imports();
    presentation_frames = av_buffer_ref(dec_frame->hw_frames_ctx);
    if (!presentation_frames) {
      fprintf(stderr, "Couldn't retain VAAPI frame pool\n");
      return;
    }
  }
  VASurfaceID decoded = (VASurfaceID)(uintptr_t)dec_frame->data[3];
  VADisplay target = device_display(presentation_ref);
  VAStatus status = vaSyncSurface(device_display(device_ref), decoded);
  if (status != VA_STATUS_SUCCESS)
    goto error;
  VASurfaceID surface;
  status = import_surface(decoded, &surface);
  if (status != VA_STATUS_SUCCESS)
    goto error;
  status = vaPutSurface(target, surface, win, 0, 0, dec_frame->width,
      dec_frame->height, 0, 0, width, height, NULL, 0, 0);
  // Keep the decoded buffer alive until the GPU has finished reading it.
  if (status == VA_STATUS_SUCCESS)
    status = vaSyncSurface(target, surface);
  if (status == VA_STATUS_SUCCESS)
    return;
error:
  fprintf(stderr, "VAAPI presentation failed: %s\n", vaErrorStr(status));
}
