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
#include <X11/Xlib.h>

#include "../place.h"

int vaapi_init_lib(Display* display);
void vaapi_cleanup_lib(void);
int vaapi_init(AVCodecContext* decoder_ctx);
// Korri: the picture goes at `place` in the window (#476).
void vaapi_queue(AVFrame* dec_frame, Window win, struct picture_place place);
// Korri: whether vaapi_queue gives this picture to EGL before it tries
// vaPutSurface. A picture with bars goes to EGL (#476); one that fills its
// window does only after picture_vaapi_egl_for_filled(1) (#391 step F5), or
// when vaPutSurface has refused this session (#477). Neither, once EGL has
// failed this session.
bool vaapi_presents_through_egl(struct picture_place place);
