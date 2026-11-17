// SPDX-License-Identifier: GPL-3.0-or-later
// The carried C picture and input code's calls into the network core,
// renamed to Korri's NetworkCore seam (src/network_core/c_calls.rs), which
// gives each call to the core that the stream runs. build.rs puts this file
// before every carried C file with -include, so those files keep Moonlight's
// names and stay as they are. The prototypes come from Limelight.h itself,
// under these names. The C checks in default.nix compile the same files
// without it, against the C core's own functions.
#pragma once
#define LiSendKeyboardEvent viewer_core_LiSendKeyboardEvent
#define LiSendMultiControllerEvent viewer_core_LiSendMultiControllerEvent
#define LiSendControllerArrivalEvent viewer_core_LiSendControllerArrivalEvent
#define LiSendMouseMoveEvent viewer_core_LiSendMouseMoveEvent
#define LiSendMousePositionEvent viewer_core_LiSendMousePositionEvent
#define LiSendMouseButtonEvent viewer_core_LiSendMouseButtonEvent
#define LiSendScrollEvent viewer_core_LiSendScrollEvent
#define LiSendHScrollEvent viewer_core_LiSendHScrollEvent
#define LiSendHighResScrollEvent viewer_core_LiSendHighResScrollEvent
#define LiSendHighResHScrollEvent viewer_core_LiSendHighResHScrollEvent
#define LiSendTouchEvent viewer_core_LiSendTouchEvent
#define LiSendControllerTouchEvent viewer_core_LiSendControllerTouchEvent
#define LiSendControllerMotionEvent viewer_core_LiSendControllerMotionEvent
#define LiGetMicroseconds viewer_core_LiGetMicroseconds
