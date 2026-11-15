# The picture on an Intel PC (#477): ibuki, 2026-11-15

ibuki is an Intel NUC8i3BEK (Iris Plus 655, iHD driver 26.2.4, libva 1.24)
with sway and Xwayland on a 3840x2160 TV. aka ran Mario Kart World and sent
1920x1080 at 60 frames each second, H.264.

## Cause

A probe on ibuki's Xwayland, as the compositor's user:

| Step | Result |
| --- | --- |
| X extensions | DRI2 absent, DRI3 present, Present present |
| `vaPutSurface` of a new surface on its own VA display | invalid parameter |
| Export a surface, import it on a second VA display | ok |
| `vaPutSurface` of the imported surface (composed or separate layers) | invalid parameter |
| Import the exported surface as an EGL image, draw it, read the pixel | ok: BT.601 red reads 255 0 0 |

So no `vaPutSurface` can present there. The import between the two VA
displays that #466 added is not the cause.

## The stream, before and after

The viewer ran by hand as the compositor's user, outside korrid, with
`-viewonly` and korrid's other arguments, against aka's live session. korrid
did not manage the window, so sway tiled it.

| Viewer | Seconds | `VAAPI presentation failed` lines | Picture |
| --- | --- | --- | --- |
| `q0dsncik…-korri-viewer-0.1.0` (`build-3a7bcbc0835f-viewer`) | 14 | 823 | none |
| This change, `ffwpqzvj…-korri-viewer-0.1.0` | 20 | 0 | the race, in the right colours (screenshot) |

With this change the log has one line, `vaPutSurface cannot present here
(invalid parameter); presenting through EGL`. Over four seconds the viewer
used 21 to 24% of one core and sway 8 to 10%.

## Not shown here

- A join through korrid, with its window and focus handling. That needs the
  published plugin.
- The frame record's numbers for this path. No measure ran.
- An AMD device. Its `vaPutSurface` path gains one flag and is otherwise as
  before; no run checked it.
- A check in the build sandbox. It has no GPU, so nothing there reaches
  `video/vaapi_egl.c` beyond compiling it with `-Wall -Wextra -Werror`.
