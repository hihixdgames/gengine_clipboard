mod format_conversion;
mod paste_data_access;

use crate::{
	ClipboardConfig, ClipboardEvent, ClipboardEventSource, InternalClipboard,
	platform::paste_data_access::WindowsDataAccess,
};
use std::{
	marker::PhantomData,
	sync::mpsc::{self, Sender},
	thread::{self, JoinHandle},
};

enum ThreadCommand {
	GetData,
	Exit,
}

pub struct Clipboard<T: ClipboardConfig> {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
	phantom: PhantomData<T>,
}

impl<T: ClipboardConfig> InternalClipboard<T> for Clipboard<T> {
	fn new(_display_handle: &dyn raw_window_handle::HasDisplayHandle, mut config: T) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(thread::spawn(move || {
			let mut event_conut = 0;

			for command in receiver {
				match command {
					ThreadCommand::GetData => {
						let source = ClipboardEventSource { value: event_conut };
						event_conut += 1;
						config.callback(ClipboardEvent::StartedPasteHandling { source });

						let mut data_access = match WindowsDataAccess::new() {
							Ok(data_access) => data_access,
							Err(error) => {
								config.callback(ClipboardEvent::FailedPasteHandling {
									source,
									error,
								});
								return;
							}
						};

						let mime_types: Vec<String> = data_access.get_mime_types();

						let event: ClipboardEvent<T::ClipboardData> =
							match T::resolve_paste_data(mime_types, &mut data_access) {
								Ok(data) => ClipboardEvent::PasteResult { source, data },
								Err(error) => ClipboardEvent::FailedPasteHandling { source, error },
							};

						config.callback(event);
					}
					ThreadCommand::Exit => break,
				}
			}
		}));

		Self {
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

impl<T: ClipboardConfig> Drop for Clipboard<T> {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.join_handle.take() {
			let _ = handle.join();
		}
	}
}
