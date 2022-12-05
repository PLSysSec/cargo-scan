
use serde::{Serialize, Deserialize};
use std::fs::*;
use std::os::*;

// nested braces
use std::fs::{mod1, mod2::{test1, test2}};
use std::{a::b::{c::d, e::f}, process::{process::Command, Command}};

// multi-line braces
use std::fs::test::{
    mod3,
    mod4
};
use std::{
    a, fs::mod5
};
use std::{
    env,
    b,
}
;

// comments
use std::fs::mod6; // comment, with, commas, in, it
use std::collections::*; // comment with import: std::fs
use std::fs::mod7;//comment-without-spaces

// example missed in rustc_version
use std::{env, error, fmt, io, num, str};
use std::{error, env}; // a variant

// Fake use expression in the comments
// from regex-automata
/*
By default, compiling a regex will use dense DFAs internally. This uses more
memory, but executes searches more quickly. If you can abide slower searches
(somewhere around 3-5x), then sparse DFAs might make more sense since they can
use significantly less space.
*/

// Multiline strings
// from crossbeam-epoch
#[deprecated(
    note = "`compare_and_set` and `compare_and_set_weak` that use this trait are deprecated, \
            use `compare_exchange` or `compare_exchange_weak instead`"
)]
pub trait CompareAndSetOrdering {
    /// The ordering of the operation when it succeeds.
    fn success(&self) -> Ordering;
}

// gigantic use expression tree
use windows_sys::Win32::{
    Devices::HumanInterfaceDevice::MOUSE_MOVE_RELATIVE,
    Foundation::{BOOL, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_TIMEOUT, WPARAM},
    Graphics::Gdi::{
        GetMonitorInfoW, GetUpdateRect, MonitorFromRect, MonitorFromWindow, RedrawWindow,
        ScreenToClient, ValidateRect, MONITORINFO, MONITOR_DEFAULTTONULL, RDW_INTERNALPAINT,
        SC_SCREENSAVE,
    },
    Media::{timeBeginPeriod, timeEndPeriod, timeGetDevCaps, TIMECAPS, TIMERR_NOERROR},
    System::{Ole::RevokeDragDrop, Threading::GetCurrentThreadId, WindowsProgramming::INFINITE},
    UI::{
        Controls::{HOVER_DEFAULT, WM_MOUSELEAVE},
        Input::{
            Ime::{GCS_COMPSTR, GCS_RESULTSTR, ISC_SHOWUICOMPOSITIONWINDOW},
            KeyboardAndMouse::{
                MapVirtualKeyA, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE,
                TRACKMOUSEEVENT,
            },
            Pointer::{
                POINTER_FLAG_DOWN, POINTER_FLAG_UP, POINTER_FLAG_UPDATE, POINTER_INFO,
                POINTER_PEN_INFO, POINTER_TOUCH_INFO,
            },
            Touch::{
                CloseTouchInputHandle, GetTouchInputInfo, TOUCHEVENTF_DOWN, TOUCHEVENTF_MOVE,
                TOUCHEVENTF_UP, TOUCHINPUT,
            },
            RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
            GetMessageW, LoadCursorW, MsgWaitForMultipleObjectsEx, PeekMessageW, PostMessageW,
            PostThreadMessageW, RegisterClassExW, RegisterWindowMessageA, SetCursor, SetWindowPos,
            TranslateMessage, CREATESTRUCTW, GIDC_ARRIVAL, GIDC_REMOVAL, GWL_STYLE, GWL_USERDATA,
            HTCAPTION, HTCLIENT, MAPVK_VK_TO_VSC, MINMAXINFO, MNC_CLOSE, MSG, MWMO_INPUTAVAILABLE,
            NCCALCSIZE_PARAMS, PM_NOREMOVE, PM_QS_PAINT, PM_REMOVE, PT_PEN, PT_TOUCH, QS_ALLEVENTS,
            RI_KEY_E0, RI_KEY_E1, RI_MOUSE_WHEEL, SC_MINIMIZE, SC_RESTORE, SIZE_MAXIMIZED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WHEEL_DELTA, WINDOWPOS,
            WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED,
            WM_DROPFILES, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_IME_COMPOSITION,
            WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION, WM_INPUT,
            WM_INPUT_DEVICE_CHANGE, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MENUCHAR, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCCREATE, WM_NCDESTROY,
            WM_NCLBUTTONDOWN, WM_PAINT, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE,
            WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TOUCH, WM_WINDOWPOSCHANGED,
            WM_WINDOWPOSCHANGING, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_OVERLAPPED, WS_POPUP,
            WS_VISIBLE,
        },
    },
};
