pub mod atoms;
pub mod paste_data_access;

use std::marker::PhantomData;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use crate::platform::x11::paste_data_access::X11DataAcess;
use crate::{ClipboardConfig, ClipboardEvent, ClipboardEventSource};

use raw_window_handle::HasDisplayHandle;
#[allow(unused_imports)]
use x11rb::protocol::xproto::{
	Atom, ConnectionExt, CreateWindowAux, EventMask, GetPropertyReply, Window, WindowClass,
	create_window, get_selection_owner, intern_atom,
};

use crate::InternalClipboard;

enum ThreadCommand {
	GetData,
	Exit,
}

pub struct X11Clipboard<T: ClipboardConfig> {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
	phantom: PhantomData<T>,
}

impl<T: ClipboardConfig> InternalClipboard<T> for X11Clipboard<T> {
	fn new(_display_handle: &dyn HasDisplayHandle, mut behaviour: T) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(thread::spawn(move || {
			let mut data_access = X11DataAcess::new();
			let mut event_conut = 0;

			for command in receiver {
				match command {
					ThreadCommand::GetData => {
						let source = ClipboardEventSource { value: event_conut };
						event_conut += 1;

						behaviour.callback(ClipboardEvent::StartedPasteHandling { source });

						let mime_types: Vec<String> = match data_access.get_mime_types() {
							Ok(mime_types) => mime_types,
							Err(error) => {
								behaviour.callback(ClipboardEvent::FailedPasteHandling {
									source,
									error,
								});
								continue;
							}
						};

						let event: ClipboardEvent<T::ClipboardData> =
							match T::resolve_paste_data(mime_types, &mut data_access) {
								Ok(data) => ClipboardEvent::PasteResult { source, data },
								Err(error) => ClipboardEvent::FailedPasteHandling { source, error },
							};

						behaviour.callback(event);
					}
					ThreadCommand::Exit => break,
				}
			}
		}));

		X11Clipboard {
			sender,
			join_handle,
			phantom: PhantomData,
		}
	}

	fn request_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, _data: ClipboardData) {
		unimplemented!("Clipboard write not implemented yet.");
	}
}

impl<T: ClipboardConfig> Drop for X11Clipboard<T> {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.join_handle.take() {
			let _ = handle.join();
		}
	}
}
