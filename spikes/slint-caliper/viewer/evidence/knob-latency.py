#!/usr/bin/env python3
"""THROWAWAY SPIKE: time from a knob change to the new frame on the screen, end to end.

Runs against the viewer on an X server (Xvfb). It types a colour into a knob's field one
key at a time through XTest. The last key makes the value valid. The clock starts when
that key is sent and stops when a probe pixel inside a frame on the screen shows the new
colour (XGetImage, polled). So the time holds the X server, winit, Slint's event
dispatch, the interpreter's property set, the software render of every frame and the
viewer window's own repaint.

  knob-latency.py FIELD_X FIELD_Y PROBE_X PROBE_Y RUNS
  (FIELD in the window's logical px at scale 1.5, PROBE in screen px)
"""
import ctypes, ctypes.util, os, sys, time

x11 = ctypes.CDLL(os.environ["LIBX11"])
xtst = ctypes.CDLL(os.environ["LIBXTST"])
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XGetImage.restype = ctypes.c_void_p
x11.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
                          ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int]
x11.XGetPixel.restype = ctypes.c_ulong
x11.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
x11.XDestroyImage = None  # a macro; leaking a 1x1 image per poll is fine for a spike
x11.XStringToKeysym.restype = ctypes.c_ulong
x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
x11.XKeysymToKeycode.restype = ctypes.c_ubyte
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]

dpy = x11.XOpenDisplay(None)
root = x11.XDefaultRootWindow(dpy)
SYM = {"#": "numbersign", "0": "0"}


def key(name, down=None):
    code = x11.XKeysymToKeycode(dpy, x11.XStringToKeysym(SYM.get(name, name).encode()))
    if down is None:
        xtst.XTestFakeKeyEvent(dpy, code, 1, 0)
        xtst.XTestFakeKeyEvent(dpy, code, 0, 0)
    else:
        xtst.XTestFakeKeyEvent(dpy, code, 1 if down else 0, 0)
    x11.XFlush(dpy)


def shift_key(name):
    key("Shift_L", True); key(name); key("Shift_L", False)


def pixel(x, y):
    img = x11.XGetImage(dpy, root, x, y, 1, 1, 0xFFFFFFFF, 2)
    p = x11.XGetPixel(img, 0, 0)
    return (p >> 16) & 255, (p >> 8) & 255, p & 255


def click(x, y):
    xtst.XTestFakeMotionEvent(dpy, 0, x, y, 0)
    xtst.XTestFakeButtonEvent(dpy, 1, 1, 0)
    xtst.XTestFakeButtonEvent(dpy, 1, 0, 0)
    x11.XFlush(dpy)


def type_value(text):
    for c in text:
        shift_key(c) if c == "#" else key(c)
        time.sleep(0.02)


def near(a, b):
    return all(abs(i - j) <= 8 for i, j in zip(a, b))


fx, fy, px, py, runs = (int(float(v)) for v in sys.argv[1:6])
values = [("#e70000", (0xE7, 0, 0)), ("#4ad39c", (0x4A, 0xD3, 0x9C))]
times = []
for run in range(runs):
    text, want = values[run % 2]
    click(round(fx * 1.5), round(fy * 1.5))
    time.sleep(0.1)
    key("Control_L", True); key("a"); key("Control_L", False)
    type_value(text[:-1])
    time.sleep(0.4)
    before = pixel(px, py)
    t0 = time.perf_counter()
    key(text[-1])
    while not near(pixel(px, py), want):
        if time.perf_counter() - t0 > 3:
            print(f"run {run}: timed out, probe {pixel(px, py)}, wanted {want}, was {before}")
            break
    else:
        ms = (time.perf_counter() - t0) * 1000
        times.append(ms)
        print(f"run {run}: {text} on screen after {ms:.1f} ms")
    time.sleep(0.3)
if times:
    times.sort()
    print(f"{len(times)} runs: median {times[len(times)//2]:.1f} ms, min {times[0]:.1f}, max {times[-1]:.1f}")
