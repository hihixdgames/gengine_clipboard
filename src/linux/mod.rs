mod wayland;
mod x11;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

use crate::{
	ClipboardConfig, InternalClipboard,
	platform::{wayland::WaylandClipboard, x11::X11Clipboard},
};

pub enum Internal<T: ClipboardConfig> {
	X11(X11Clipboard<T>),
	Wayland(WaylandClipboard<T>),
}

pub struct Clipboard<T: ClipboardConfig> {
	internal: Internal<T>,
}

impl<T: ClipboardConfig> InternalClipboard<T> for Clipboard<T> {
	fn new(display_handle: &dyn HasDisplayHandle, behaviour: T) -> Self {
		let handle = display_handle.display_handle().unwrap();
		match handle.as_raw() {
			RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
				println!("Using X11");
				Clipboard {
					internal: Internal::X11(X11Clipboard::new(display_handle, behaviour)),
				}
			}
			RawDisplayHandle::Wayland(_) => {
				println!("Using wayland!");
				Clipboard {
					internal: Internal::Wayland(WaylandClipboard::new(display_handle, behaviour)),
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
