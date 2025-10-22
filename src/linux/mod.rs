mod wayland;
mod x11;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

use crate::{
	ClipboardHandler, InternalClipboard,
	platform::{wayland::WaylandClipboard, x11::X11Clipboard},
};

pub enum Internal {
	X11(X11Clipboard),
	Wayland(WaylandClipboard),
}

pub struct Clipboard {
	internal: Internal,
}

impl InternalClipboard for Clipboard {
	fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self {
		let handle = display_handle.display_handle().unwrap();
		match handle.as_raw() {
			RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
				println!("Using X11");
				Clipboard {
					internal: Internal::X11(X11Clipboard::new(display_handle, handler)),
				}
			}
			RawDisplayHandle::Wayland(_) => {
				println!("Using wayland!");
				Clipboard {
					internal: Internal::Wayland(WaylandClipboard::new(display_handle, handler)),
				}
			}
			_ => panic!(),
		}
	}

	fn request_data(&self) {
		match &self.internal {
			Internal::X11(internal) => {
				internal.request_data();
			}
			Internal::Wayland(internal) => {
				internal.request_data();
			}
		}
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
