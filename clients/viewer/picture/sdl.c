/*
 * This file is part of Moonlight Embedded.
 *
 * Copyright (C) 2015 Iwan Timmer
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

#ifdef HAVE_SDL

#include "sdl.h"
#include "frame_times.h"
#include "place.h"
#include "../io/bridge.h"

#include <Limelight.h>
#include <libavutil/frame.h>

static bool done;
static int fullscreen_flags;

static SDL_Window *window;
static SDL_Renderer *renderer;
static SDL_Texture *bmp;

SDL_mutex *mutex;

int sdlCurrentFrame, sdlNextFrame;

// Korri (#476): the texture's size, which follows the stream's.
static int texture_width, texture_height;

static bool make_texture(int width, int height) {
  SDL_Texture* texture = SDL_CreateTexture(renderer, SDL_PIXELFORMAT_YV12, SDL_TEXTUREACCESS_STREAMING, width, height);
  if (!texture) {
    fprintf(stderr, "SDL: could not create a %dx%d texture - %s\n", width, height, SDL_GetError());
    return false;
  }
  if (bmp)
    SDL_DestroyTexture(bmp);
  bmp = texture;
  texture_width = width;
  texture_height = height;
  return true;
}

void sdl_init(int width, int height, bool fullscreen) {
  sdlCurrentFrame = sdlNextFrame = 0;
  done = false;

  if(SDL_Init(SDL_INIT_VIDEO | SDL_INIT_EVENTS)) {
    fprintf(stderr, "Could not initialize SDL - %s\n", SDL_GetError());
    exit(1);
  }

  fullscreen_flags = fullscreen?SDL_WINDOW_FULLSCREEN:0;
  window = SDL_CreateWindow("Moonlight", SDL_WINDOWPOS_UNDEFINED, SDL_WINDOWPOS_UNDEFINED, width, height, SDL_WINDOW_OPENGL | fullscreen_flags);
  if(!window) {
    fprintf(stderr, "SDL: could not create window - exiting\n");
    exit(1);
  }

  renderer = SDL_CreateRenderer(window, -1, SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
  if (!renderer) {
    printf("SDL_CreateRenderer failed: %s\n", SDL_GetError());
    exit(1);
  }

  if (!make_texture(width, height)) {
    fprintf(stderr, "SDL: could not create texture - exiting\n");
    exit(1);
  }

  mutex = SDL_CreateMutex();
  if (!mutex) {
    fprintf(stderr, "Couldn't create mutex\n");
    exit(1);
  }
}

int sdl_poll(int timeout) {
  SDL_Event event;

  if (!done && SDL_WaitEventTimeout(&event, timeout)) {
    switch (viewer_input_sdl(window, &event)) {
    case SDL_QUIT_APPLICATION:
      done = true;
      break;
    case SDL_TOGGLE_FULLSCREEN:
      fullscreen_flags ^= SDL_WINDOW_FULLSCREEN;
      SDL_SetWindowFullscreen(window, fullscreen_flags);
      break;
    case SDL_MOUSE_GRAB:
      SDL_ShowCursor(SDL_ENABLE);
      SDL_SetRelativeMouseMode(SDL_TRUE);
      break;
    case SDL_MOUSE_UNGRAB:
      SDL_SetRelativeMouseMode(SDL_FALSE);
      SDL_ShowCursor(SDL_DISABLE);
      break;
    default:
      if (event.type == SDL_QUIT)
        done = true;
      else if (event.type == SDL_USEREVENT) {
        if (event.user.code == SDL_CODE_FRAME) {
          if (++sdlCurrentFrame <= sdlNextFrame - SDL_BUFFER_FRAMES) {
            //Skip frame
          } else if (SDL_LockMutex(mutex) == 0) {
            AVFrame* frame = event.user.data1;
            Uint8** data = frame->data;
            int* linesize = frame->linesize;
            int64_t pts = frame->pts;
            int width = frame->width, height = frame->height;
            // The stream's size can change in the middle of the stream
            // (Korri #476): the texture takes each new size.
            if ((width != texture_width || height != texture_height) && !make_texture(width, height)) {
              SDL_UnlockMutex(mutex);
              break;
            }
            SDL_UpdateYUVTexture(bmp, NULL, data[0], linesize[0], data[1], linesize[1], data[2], linesize[2]);
            SDL_UnlockMutex(mutex);
            // The picture fits the window with its own shape, black around it.
            int window_width = 0, window_height = 0;
            SDL_GetRendererOutputSize(renderer, &window_width, &window_height);
            struct picture_place place = picture_fit(width, height, window_width, window_height);
            SDL_Rect shown = {place.x, place.y, place.width, place.height};
            SDL_SetRenderDrawColor(renderer, 0, 0, 0, 255);
            SDL_RenderClear(renderer);
            SDL_RenderCopy(renderer, bmp, NULL, &shown);
            SDL_RenderPresent(renderer);
            frame_times_displayed(pts);
            picture_shown(width, height);
          } else
            fprintf(stderr, "Couldn't lock mutex\n");
        }
      }
    }
  }

  return done;
}

void sdl_destroy(void) {
  SDL_DestroyTexture(bmp);
  bmp = NULL;
  texture_width = texture_height = 0;
  SDL_DestroyRenderer(renderer);
  SDL_DestroyMutex(mutex);
  SDL_DestroyWindow(window);
  SDL_Quit();
}

#endif /* HAVE_SDL */
