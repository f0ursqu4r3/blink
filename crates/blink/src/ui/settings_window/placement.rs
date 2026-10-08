//! Place settings on the display that contains the pointer.

use gpui_kit::*;

fn centered(visible: Bounds<Pixels>, desired: Size<Pixels>) -> Bounds<Pixels> {
    let size = desired.min(&visible.size);
    Bounds::new(
        visible.center() - point(size.width / 2., size.height / 2.),
        size,
    )
}

pub(super) fn settings_bounds(window: &Window, cx: &App) -> (Option<DisplayId>, Bounds<Pixels>) {
    let pointer_display = pointer_display();
    let display = cx
        .displays()
        .into_iter()
        .find(|display| Some(display.id()) == pointer_display)
        .or_else(|| window.display(cx));
    let desired = size(px(760.), px(720.));
    match display {
        Some(display) => (
            Some(display.id()),
            centered(display.visible_bounds(), desired),
        ),
        None => (None, Bounds::centered(None, desired, cx)),
    }
}

#[cfg(target_os = "macos")]
fn pointer_screen() -> Option<objc2::rc::Retained<objc2_app_kit::NSScreen>> {
    let main_thread = objc2::MainThreadMarker::new()?;
    let pointer = objc2_app_kit::NSEvent::mouseLocation();
    objc2_app_kit::NSScreen::screens(main_thread)
        .iter()
        .find(|screen| {
            let frame = screen.frame();
            pointer.x >= frame.origin.x
                && pointer.x < frame.origin.x + frame.size.width
                && pointer.y >= frame.origin.y
                && pointer.y < frame.origin.y + frame.size.height
        })
}

#[cfg(target_os = "macos")]
fn pointer_display() -> Option<DisplayId> {
    // GPUI's macOS display bounds are display-relative. Resolve the native
    // screen first, rather than comparing them to a global pointer position.
    let description = pointer_screen()?.deviceDescription();
    let number =
        description.objectForKey(&objc2_foundation::NSString::from_str("NSScreenNumber"))?;
    let number = number.downcast_ref::<objc2_foundation::NSNumber>()?;
    Some(DisplayId::new(number.unsignedIntValue() as u64))
}

/// Opening settings again moves the existing window without losing field edits.
#[cfg(target_os = "macos")]
pub(super) fn center_existing(window: &mut Window) {
    use raw_window_handle::RawWindowHandle;
    let Some(screen) = pointer_screen() else {
        return;
    };
    let Ok(handle) = <Window as raw_window_handle::HasWindowHandle>::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // SAFETY: GPUI owns this live NSView for the duration of the window borrow.
    // pointer_screen also established that this call runs on the main thread.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>() };
    let Some(native) = view.window() else {
        return;
    };
    let visible = screen.visibleFrame();
    let mut frame = native.frame();
    frame.size.width = frame.size.width.min(visible.size.width);
    frame.size.height = frame.size.height.min(visible.size.height);
    frame.origin.x = visible.origin.x + (visible.size.width - frame.size.width) / 2.;
    frame.origin.y = visible.origin.y + (visible.size.height - frame.size.height) / 2.;
    // GPUI resize queues an asynchronous setContentSize. Set size and position
    // together here, so centering uses the final frame on a smaller display.
    native.setFrame_display(frame, true);
}

#[cfg(not(target_os = "macos"))]
fn pointer_display() -> Option<DisplayId> {
    None
}

#[cfg(not(target_os = "macos"))]
pub(super) fn center_existing(_: &mut Window) {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn centers_on_offset_displays_and_fits_small_work_areas() {
        let bounds = centered(
            Bounds::new(point(px(-1440.), px(25.)), size(px(1440.), px(875.))),
            size(px(760.), px(720.)),
        );
        assert_eq!(bounds.origin, point(px(-1100.), px(102.5)));
        let bounds = centered(
            Bounds::new(point(px(1920.), px(-900.)), size(px(640.), px(480.))),
            size(px(760.), px(720.)),
        );
        assert_eq!(bounds.origin, point(px(1920.), px(-900.)));
        assert_eq!(bounds.size, size(px(640.), px(480.)));
    }
}
