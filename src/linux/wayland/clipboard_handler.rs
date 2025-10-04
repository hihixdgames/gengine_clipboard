use sctk::{
	data_device_manager::{
		DataDeviceManagerState, ReadPipe,
		data_device::{DataDevice, DataDeviceHandler},
		data_offer::DataOfferHandler,
		data_source::DataSourceHandler,
	},
	delegate_data_device, delegate_pointer, delegate_registry, delegate_seat, delegate_touch,
	reexports::{
		calloop::{LoopHandle, PostAction},
		calloop_wayland_source::WaylandSource,
		client::{
			Connection, Dispatch, Proxy,
			backend::{Backend, ObjectId},
			globals::registry_queue_init,
			protocol::{
				wl_keyboard::{self, WlKeyboard},
				wl_pointer::WlPointer,
				wl_touch::WlTouch,
			},
		},
	},
	registry::{ProvidesRegistryState, RegistryState},
	registry_handlers,
	seat::{
		SeatHandler, SeatState,
		pointer::{PointerData, PointerEventKind, PointerHandler},
		touch::{TouchData, TouchHandler},
	},
};
use std::{
	borrow::Cow,
	collections::HashMap,
	io::{ErrorKind, Read},
	mem,
	os::fd::AsRawFd,
};

use crate::{
	ClipboardCallback, ClipboardError, ClipboardEvent, platform::wayland::mime_type::MimeType,
};

#[derive(Default)]
struct SeatCapabilities {
	keyboard: Option<WlKeyboard>,
	pointer: Option<WlPointer>,
	touch: Option<WlTouch>,
	data_device: Option<DataDevice>,
}

impl Drop for SeatCapabilities {
	fn drop(&mut self) {
		if let Some(keyboard) = self.keyboard.take() {
			keyboard.release();
		}

		if let Some(pointer) = self.pointer.take() {
			pointer.release();
		}

		if let Some(touch) = self.touch.take() {
			touch.release();
		}
	}
}

pub struct ClipboardHandler<F: ClipboardCallback> {
	callback: F,
	registry_state: RegistryState,
	seat_state: SeatState,
	data_device_manager_state: Option<DataDeviceManagerState>,
	seats: HashMap<ObjectId, SeatCapabilities>,
	latest_seat: Option<ObjectId>,
	loop_handle: LoopHandle<'static, Self>,
	pub exit: bool,
}

impl<F: ClipboardCallback> ClipboardHandler<F> {
	pub fn create_and_insert(
		backend: Backend,
		loop_handle: LoopHandle<'static, Self>,
		callback: F,
	) -> Self {
		let connection = Connection::from_backend(backend);
		let (globals, event_queue) =
			registry_queue_init::<ClipboardHandler<F>>(&connection).unwrap();
		let queue_handle = &event_queue.handle();

		let data_device_manager_state = DataDeviceManagerState::bind(&globals, queue_handle).ok();
		let seat_state = SeatState::new(&globals, queue_handle);

		#[allow(clippy::mutable_key_type)]
		let mut seats = HashMap::new();
		for seat in seat_state.seats() {
			seats.insert(seat.id(), Default::default());
		}

		WaylandSource::new(connection, event_queue)
			.insert(loop_handle.clone())
			.unwrap();

		Self {
			registry_state: RegistryState::new(&globals),
			seat_state,
			data_device_manager_state,
			seats,
			latest_seat: None,
			exit: false,
			callback,
			loop_handle,
		}
	}

	pub fn request_data(&mut self) {
		(self.callback)(ClipboardEvent::StartedPasteHandling);

		let latest = match self.latest_seat.as_ref() {
			Some(latest) => latest,
			_ => {
				(self.callback)(ClipboardEvent::FailedPasteHandling(
					ClipboardError::Unknown("No latest in wayland".to_string()),
				));
				return;
			}
		};

		let seat = match self.seats.get_mut(latest) {
			Some(seat) => seat,
			_ => {
				(self.callback)(ClipboardEvent::FailedPasteHandling(
					ClipboardError::Unknown("Latest seat not available in wayland".to_string()),
				));
				return;
			}
		};

		let selection = match seat.data_device.as_ref() {
			Some(data_device) => match data_device.data().selection_offer() {
				Some(selection) => selection,
				_ => {
					(self.callback)(ClipboardEvent::FailedPasteHandling(ClipboardError::Empty));
					return;
				}
			},
			_ => {
				(self.callback)(ClipboardEvent::FailedPasteHandling(
					ClipboardError::Unknown("no data device in wayland".to_string()),
				));
				return;
			}
		};

		let mime_type = match selection
			.with_mime_types(|offers| MimeType::select(MimeType::DEFAULT_TARGETS, offers))
		{
			Some(mime_type) => mime_type,
			_ => {
				(self.callback)(ClipboardEvent::FailedPasteHandling(
					ClipboardError::UnsupportedMimeType,
				));
				return;
			}
		};

		let read_pipe = match selection.receive(mime_type.as_str().to_string()) {
			Ok(read_pipe) => read_pipe,
			_ => {
				(self.callback)(ClipboardEvent::FailedPasteHandling(
					ClipboardError::Unknown(
						"selection does not want to give after offering wayland".to_string(),
					),
				));
				return;
			}
		};

		if set_non_blocking(&read_pipe).is_err() {
			(self.callback)(ClipboardEvent::FailedPasteHandling(
				ClipboardError::Unknown("Failed t oset to non locking wayland".to_string()),
			));
			return;
		}

		let mut reader_buffer = [0; 4096];
		let mut content = Vec::new();
		let _ = self
			.loop_handle
			.insert_source(read_pipe, move |_, file, state| {
				let file = unsafe { file.get_mut() };
				loop {
					match file.read(&mut reader_buffer) {
						Ok(0) => {
							if mime_type.is_string() {
								let string =
									if let Cow::Owned(string) = String::from_utf8_lossy(&content) {
										string
									} else {
										// Not owned means that it is valid.
										let mut content_copy = Vec::new();
										// This is needed to make the closure safe
										mem::swap(&mut content, &mut content_copy);
										String::from_utf8(content_copy).unwrap()
									};

								// Maybe normalize like smithay clipboad?

								(state.callback)(ClipboardEvent::Paste(
									crate::ClipboardData::Text(crate::Text::Plain(string)),
									None,
								));
								return PostAction::Remove;
							}
							println!("Not yet implemented");
							todo!()
						}
						Ok(n) => content.extend_from_slice(&reader_buffer[..n]),
						Err(err) if err.kind() == ErrorKind::WouldBlock => {
							return PostAction::Continue;
						}
						Err(_) => {
							(state.callback)(ClipboardEvent::FailedPasteHandling(
								ClipboardError::Unknown("Failed to read file wayland".to_string()),
							));
							return PostAction::Remove;
						}
					};
				}
			});
	}
}

impl<F: ClipboardCallback> SeatHandler for ClipboardHandler<F> {
	fn new_seat(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		seat: sctk::reexports::client::protocol::wl_seat::WlSeat,
	) {
		self.seats.insert(seat.id(), Default::default());
	}

	fn remove_seat(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		seat: sctk::reexports::client::protocol::wl_seat::WlSeat,
	) {
		self.seats.remove(&seat.id());
	}

	fn new_capability(
		&mut self,
		_conn: &Connection,
		qh: &sctk::reexports::client::QueueHandle<Self>,
		seat: sctk::reexports::client::protocol::wl_seat::WlSeat,
		capability: sctk::seat::Capability,
	) {
		let seat_capabilities = self.seats.get_mut(&seat.id()).unwrap();

		use sctk::seat::Capability;
		match capability {
			Capability::Keyboard => {
				seat_capabilities.keyboard = Some(seat.get_keyboard(qh, seat.id()));

				if seat_capabilities.data_device.is_none()
					&& self.data_device_manager_state.is_some()
				{
					seat_capabilities.data_device = self
						.data_device_manager_state
						.as_ref()
						.map(|manager| manager.get_data_device(qh, &seat));
				}
			}
			Capability::Pointer => {
				seat_capabilities.pointer = self.seat_state.get_pointer(qh, &seat).ok();
			}
			Capability::Touch => {
				seat_capabilities.touch = self.seat_state.get_touch(qh, &seat).ok();
			}
			_ => {}
		}
	}

	fn remove_capability(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		seat: sctk::reexports::client::protocol::wl_seat::WlSeat,
		capability: sctk::seat::Capability,
	) {
		let capabilities = self.seats.get_mut(&seat.id()).unwrap();
		use sctk::seat::Capability;
		match capability {
			Capability::Keyboard => {
				capabilities.data_device = None;

				if let Some(keyboard) = capabilities.keyboard.take() {
					keyboard.release();
				}
			}
			Capability::Pointer => {
				if let Some(pointer) = capabilities.keyboard.take() {
					pointer.release();
				}
			}
			Capability::Touch => {
				if let Some(touch) = capabilities.touch.take() {
					touch.release();
				}
			}
			_ => {}
		}
	}

	fn seat_state(&mut self) -> &mut SeatState {
		&mut self.seat_state
	}
}

impl<F: ClipboardCallback> DataDeviceHandler for ClipboardHandler<F> {
	fn drop_performed(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_data_device: &sctk::reexports::client::protocol::wl_data_device::WlDataDevice,
	) {
	}

	fn enter(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_data_device: &sctk::reexports::client::protocol::wl_data_device::WlDataDevice,
		_x: f64,
		_y: f64,
		_wl_surface: &sctk::reexports::client::protocol::wl_surface::WlSurface,
	) {
	}

	fn leave(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_data_device: &sctk::reexports::client::protocol::wl_data_device::WlDataDevice,
	) {
	}

	fn motion(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_data_device: &sctk::reexports::client::protocol::wl_data_device::WlDataDevice,
		_x: f64,
		_y: f64,
	) {
	}

	fn selection(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_data_device: &sctk::reexports::client::protocol::wl_data_device::WlDataDevice,
	) {
	}
}

impl<F: ClipboardCallback> DataOfferHandler for ClipboardHandler<F> {
	fn selected_action(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_offer: &mut sctk::data_device_manager::data_offer::DragOffer,
		_actions: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
	) {
	}

	fn source_actions(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_offer: &mut sctk::data_device_manager::data_offer::DragOffer,
		_actions: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
	) {
	}
}

impl<F: ClipboardCallback> DataSourceHandler for ClipboardHandler<F> {
	fn send_request(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
		_mime: String,
		_fd: sctk::data_device_manager::WritePipe,
	) {
	}

	fn accept_mime(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
		_mime: Option<String>,
	) {
	}

	fn dnd_dropped(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
	) {
	}

	fn action(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
		_action: sctk::reexports::client::protocol::wl_data_device_manager::DndAction,
	) {
	}

	fn cancelled(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
	) {
	}

	fn dnd_finished(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_source: &sctk::reexports::client::protocol::wl_data_source::WlDataSource,
	) {
	}
}

impl<F: ClipboardCallback> ProvidesRegistryState for ClipboardHandler<F> {
	registry_handlers![SeatState];

	fn registry(&mut self) -> &mut RegistryState {
		&mut self.registry_state
	}
}

impl<F: ClipboardCallback> Dispatch<WlKeyboard, ObjectId, ClipboardHandler<F>>
	for ClipboardHandler<F>
{
	fn event(
		state: &mut ClipboardHandler<F>,
		_proxy: &WlKeyboard,
		event: <WlKeyboard as Proxy>::Event,
		data: &ObjectId,
		_conn: &Connection,
		_qhandle: &sctk::reexports::client::QueueHandle<ClipboardHandler<F>>,
	) {
		match event {
			wl_keyboard::Event::Key { .. } | wl_keyboard::Event::Modifiers { .. } => {
				state.latest_seat = Some(data.clone());
			}
			_ => {}
		}
	}
}

impl<F: ClipboardCallback> TouchHandler for ClipboardHandler<F> {
	fn down(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		touch: &WlTouch,
		_serial: u32,
		_time: u32,
		_surface: sctk::reexports::client::protocol::wl_surface::WlSurface,
		_id: i32,
		_position: (f64, f64),
	) {
		let seat = touch.data::<TouchData>().unwrap().seat();
		self.latest_seat = Some(seat.id());
	}

	fn up(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		touch: &WlTouch,
		_serial: u32,
		_time: u32,
		_id: i32,
	) {
		let seat = touch.data::<TouchData>().unwrap().seat();
		self.latest_seat = Some(seat.id());
	}

	fn cancel(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_touch: &WlTouch,
	) {
	}

	fn motion(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_touch: &WlTouch,
		_time: u32,
		_id: i32,
		_position: (f64, f64),
	) {
	}

	fn orientation(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_touch: &WlTouch,
		_id: i32,
		_orientation: f64,
	) {
	}

	fn shape(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		_touch: &WlTouch,
		_id: i32,
		_major: f64,
		_minor: f64,
	) {
	}
}

impl<F: ClipboardCallback> PointerHandler for ClipboardHandler<F> {
	fn pointer_frame(
		&mut self,
		_conn: &Connection,
		_qh: &sctk::reexports::client::QueueHandle<Self>,
		pointer: &WlPointer,
		events: &[sctk::seat::pointer::PointerEvent],
	) {
		for event in events {
			match event.kind {
				PointerEventKind::Press { .. } | PointerEventKind::Release { .. } => {
					let seat = pointer.data::<PointerData>().unwrap().seat();
					self.latest_seat = Some(seat.id());
					return;
				}
				_ => (),
			}
		}
	}
}

delegate_seat!(@<F: ClipboardCallback> ClipboardHandler<F>);
delegate_touch!(@<F: ClipboardCallback> ClipboardHandler<F>);
delegate_pointer!(@<F: ClipboardCallback> ClipboardHandler<F>);
delegate_data_device!(@<F: ClipboardCallback> ClipboardHandler<F>);
delegate_registry!(@<F: ClipboardCallback> ClipboardHandler<F>);

/// simthay-clipboard uses this so we trust that it is correct to use it
/// with tools made by them.
fn set_non_blocking(read_pipe: &ReadPipe) -> std::io::Result<()> {
	let raw_fd = read_pipe.as_raw_fd();

	let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFL) };

	if flags < 0 {
		return Err(std::io::Error::last_os_error());
	}

	let result = unsafe { libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };

	if result < 0 {
		return Err(std::io::Error::last_os_error());
	}

	Ok(())
}
