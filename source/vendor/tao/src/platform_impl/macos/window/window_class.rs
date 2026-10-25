// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// Agent HUD patch: create the per-window Panel class before allocating it.
use objc2::{
    msg_send,
    runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
};
use objc2_app_kit::{NSEvent, NSEventType, NSWindow};
use objc2_foundation::NSRect;
use std::sync::LazyLock;

struct WindowClass(&'static AnyClass);
static WINDOW_CLASS: LazyLock<WindowClass> = LazyLock::new(|| declare(false));
static PANEL_CLASS: LazyLock<WindowClass> = LazyLock::new(|| declare(true));
fn declare(panel: bool) -> WindowClass {
    let (name, parent) = if panel {
        (c"TaoNonactivatingPanel", objc2::class!(NSPanel))
    } else {
        (c"TaoWindow", objc2::class!(NSWindow))
    };
    let mut class = ClassBuilder::new(name, parent).expect("Tao native window class is unique");
    unsafe {
        class.add_method(
            objc2::sel!(canBecomeMainWindow),
            if panel {
                never_main as extern "C" fn(_, _) -> _
            } else {
                is_focusable as extern "C" fn(_, _) -> _
            },
        );
        class.add_method(
            objc2::sel!(canBecomeKeyWindow),
            is_focusable as extern "C" fn(_, _) -> _,
        );
        class.add_method(
            objc2::sel!(sendEvent:),
            if panel {
                send_panel_event as extern "C" fn(_, _, _)
            } else {
                send_window_event as extern "C" fn(_, _, _)
            },
        );
        // Both classes own this ivar. Its offset is determined by its actual superclass,
        // and every access resolves that offset by name instead of assuming a layout.
        class.add_ivar::<Bool>(c"focusable");
        if panel {
            class.add_method(
                objc2::sel!(constrainFrameRect:toScreen:),
                unconstrained as extern "C" fn(_, _, _, _) -> _,
            );
            class.add_method(
                objc2::sel!(resignKeyWindow),
                resign_key as extern "C" fn(_, _),
            );
        }
    }
    WindowClass(class.register())
}
pub(super) fn window_class(panel: bool) -> &'static AnyClass {
    if panel {
        PANEL_CLASS.0
    } else {
        WINDOW_CLASS.0
    }
}
extern "C" fn never_main(_this: &AnyObject, _selector: Sel) -> Bool {
    Bool::NO
}
#[allow(deprecated)]
extern "C" fn is_focusable(this: &AnyObject, _selector: Sel) -> Bool {
    unsafe { *this.get_ivar("focusable") }
}
#[allow(deprecated)]
pub(super) fn set_focusable(window: &NSWindow, focusable: bool) {
    unsafe {
        let object = (window as *const NSWindow).cast_mut().cast::<AnyObject>();
        *(*object).get_mut_ivar::<Bool>("focusable") = focusable.into();
    }
}
extern "C" fn unconstrained(
    _this: &AnyObject,
    _selector: Sel,
    frame: NSRect,
    _screen: *mut AnyObject,
) -> NSRect {
    frame
}
#[allow(deprecated)]
extern "C" fn resign_key(this: &AnyObject, _selector: Sel) {
    unsafe {
        // Bind super to the class that declares this method. AppKit/KVO may
        // insert a runtime subclass above ours; its direct superclass would
        // select this same implementation again and recurse.
        let _: () = msg_send![super(this, objc2::class!(NSPanel)), resignKeyWindow];
        let object = (this as *const AnyObject).cast_mut();
        if bool::from(*this.get_ivar::<Bool>("focusable")) {
            *(*object).get_mut_ivar::<Bool>("focusable") = Bool::NO;
            let _: bool = msg_send![this,makeFirstResponder:std::ptr::null::<AnyObject>()];
        }
    }
}
extern "C" fn send_window_event(this: &AnyObject, _selector: Sel, event: &NSEvent) {
    send_event(this, event, objc2::class!(NSWindow));
}
extern "C" fn send_panel_event(this: &AnyObject, _selector: Sel, event: &NSEvent) {
    send_event(this, event, objc2::class!(NSPanel));
}
fn send_event(this: &AnyObject, event: &NSEvent, superclass: &AnyClass) {
    unsafe {
        if event.r#type() == NSEventType::LeftMouseDown {
            let movable: bool = msg_send![this, isMovableByWindowBackground];
            if movable {
                let _: () = msg_send![this,performWindowDragWithEvent:event];
            }
        }
        let _: () = msg_send![super(this,superclass),sendEvent:event];
    }
}
