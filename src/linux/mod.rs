mod wayland;
mod x11;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

use crate::{
	InternalClipboard,
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
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend + 'static>(
		display_handle: &dyn HasDisplayHandle,
		callback: F,
	) -> Self {
		let handle = display_handle.display_handle().unwrap();
		match handle.as_raw() {
			RawDisplayHandle::Xlib(_) => {
				println!("Using X11");
				todo!()
			}
			RawDisplayHandle::Wayland(_) => {
				println!("Using wayland!");
				Clipboard {
					internal: Internal::Wayland(WaylandClipboard::new(display_handle, callback)),
				}
			}
			_ => unreachable!(),
		}
	}

	fn request_data(&self) {
		match &self.internal {
			Internal::X11(_) => {
				todo!()
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
