mod clipboard_handler;
mod paste_data_access;

use std::{
	marker::PhantomData,
	thread::{self, JoinHandle},
};

use raw_window_handle::RawDisplayHandle;
use sctk::reexports::{
	calloop::{
		EventLoop,
		channel::{self, Sender, channel},
	},
	client::backend::Backend,
};

use crate::{
	ClipboardConfig, InternalClipboard, platform::wayland::clipboard_handler::ClipboardHandler,
};

pub struct WaylandClipboard<T: ClipboardConfig> {
	sender: Sender<ThreadCommand>,
	handle: Option<JoinHandle<()>>,
	phantom_data: PhantomData<T>,
}

pub enum ThreadCommand {
	RequestData,
	Exit,
}

impl<T: ClipboardConfig> InternalClipboard<T> for WaylandClipboard<T> {
	fn new(window_handle: &dyn raw_window_handle::HasDisplayHandle, behaviour: T) -> Self {
		let (sender, receiver) = channel::<ThreadCommand>();

		let display_handle = window_handle.display_handle().unwrap();
		let display = if let RawDisplayHandle::Wayland(handle) = display_handle.as_raw() {
			handle.display
		} else {
			unreachable!()
		};
		let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };

		let handle = thread::spawn(move || {
			let mut event_loop = EventLoop::<ClipboardHandler<T>>::try_new().unwrap();
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

			let mut handler =
				ClipboardHandler::<T>::create_and_insert(backend, loop_handle.clone(), behaviour);

			loop {
				if event_loop.dispatch(None, &mut handler).is_err() || handler.exit {
					break;
				}
			}
		});

		Self {
			sender,
			handle: Some(handle),
			phantom_data: PhantomData,
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

impl<T: ClipboardConfig> Drop for WaylandClipboard<T> {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
	}
}
