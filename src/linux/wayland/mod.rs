mod clipboard_handler;
mod even_handler_thread;
mod paste_data_access;

use std::thread::{self, JoinHandle};

use raw_window_handle::RawDisplayHandle;
use sctk::reexports::{
	calloop::{
		EventLoop,
		channel::{self, Sender, channel},
	},
	client::backend::Backend,
};

use crate::{
	ClipboardHandler, InternalClipboard, platform::wayland::clipboard_handler::WaylandHandler,
};

pub use paste_data_access::WaylandPasteDataAccess;

pub struct WaylandClipboard {
	sender: Sender<ThreadCommand>,
	handle: Option<JoinHandle<()>>,
}

pub enum ThreadCommand {
	RequestData,
	Exit,
}

impl InternalClipboard for WaylandClipboard {
	fn new<T: ClipboardHandler>(
		window_handle: &dyn raw_window_handle::HasDisplayHandle,
		handler: T,
	) -> Self {
		let (sender, receiver) = channel::<ThreadCommand>();

		let display_handle = window_handle.display_handle().unwrap();
		let display = if let RawDisplayHandle::Wayland(handle) = display_handle.as_raw() {
			handle.display
		} else {
			unreachable!()
		};
		let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };

		let handle = thread::spawn(move || {
			let mut event_loop = EventLoop::<WaylandHandler>::try_new().unwrap();
			let loop_handle = event_loop.handle();
			loop_handle
				.insert_source(receiver, |event, _, state| {
					if let channel::Event::Msg(event) = event {
						match event {
							ThreadCommand::RequestData => {
								state.request_data();
							}
							ThreadCommand::Exit => state.exit = true,
						}
					}
				})
				.unwrap();

			let mut wayland_handler =
				WaylandHandler::create_and_insert(backend, loop_handle.clone(), handler);

			loop {
				if event_loop.dispatch(None, &mut wayland_handler).is_err() || wayland_handler.exit
				{
					break;
				}
			}
		});

		Self {
			sender,
			handle: Some(handle),
		}
	}

	fn request_data(&self) {
		let _ = self.sender.send(ThreadCommand::RequestData);
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, _data: ClipboardData) {
		unimplemented!("Clipboard write not implemented yet.");
	}
}

impl Drop for WaylandClipboard {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
	}
}
