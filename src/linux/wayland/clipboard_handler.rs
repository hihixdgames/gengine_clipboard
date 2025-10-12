use sctk::{
	data_device_manager::{
		DataDeviceManagerState,
		data_device::{DataDevice, DataDeviceHandler},
		data_offer::DataOfferHandler,
		data_source::DataSourceHandler,
	},
	delegate_data_device, delegate_pointer, delegate_registry, delegate_seat, delegate_touch,
	reexports::{
		calloop::{self, LoopHandle},
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
use std::{collections::HashMap, thread::spawn};

use crate::{
	ClipboardConfig, ClipboardError, ClipboardEvent, ClipboardEventSource,
	platform::wayland::paste_data_access::WaylandPasteDataAccess,
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

pub struct ClipboardHandler<T: ClipboardConfig> {
	behaviour: T,
	registry_state: RegistryState,
	seat_state: SeatState,
	data_device_manager_state: Option<DataDeviceManagerState>,
	seats: HashMap<ObjectId, SeatCapabilities>,
	latest_seat: Option<ObjectId>,
	loop_handle: LoopHandle<'static, Self>,
	even_count: usize,
	pub exit: bool,
}

impl<T: ClipboardConfig> ClipboardHandler<T> {
	pub fn create_and_insert(
		backend: Backend,
		loop_handle: LoopHandle<'static, Self>,
		behaviour: T,
	) -> Self {
		let connection = Connection::from_backend(backend);
		let (globals, event_queue) =
			registry_queue_init::<ClipboardHandler<T>>(&connection).unwrap();
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
			behaviour,
			loop_handle,
			even_count: 0,
		}
	}

	pub fn request_data(&mut self) {
		let source = ClipboardEventSource {
			value: self.even_count,
		};
		self.even_count += 1;

		self.behaviour
			.callback(ClipboardEvent::StartedPasteHandling { source });

		let latest = match self.latest_seat.as_ref() {
			Some(latest) => latest,
			_ => {
				self.behaviour
					.callback(ClipboardEvent::FailedPasteHandling {
						source,
						error: ClipboardError::Unknown("No latest in wayland".to_string()),
					});
				return;
			}
		};

		let seat = match self.seats.get_mut(latest) {
			Some(seat) => seat,
			_ => {
				self.behaviour
					.callback(ClipboardEvent::FailedPasteHandling {
						source,
						error: ClipboardError::Unknown(
							"Latest seat not available in wayland".to_string(),
						),
					});
				return;
			}
		};

		let selection = match seat.data_device.as_ref() {
			Some(data_device) => match data_device.data().selection_offer() {
				Some(selection) => selection,
				_ => {
					self.behaviour
						.callback(ClipboardEvent::FailedPasteHandling {
							source,
							error: ClipboardError::Empty,
						});
					return;
				}
			},
			_ => {
				self.behaviour
					.callback(ClipboardEvent::FailedPasteHandling {
						source,
						error: ClipboardError::Unknown("no data device in wayland".to_string()),
					});
				return;
			}
		};

		let (sender, channel) = calloop::channel::channel();
		let handle = spawn(move || {
			let mime_types = selection.with_mime_types(|offers| offers.to_vec());
			let mut data_access = WaylandPasteDataAccess::new(selection);

			let event: ClipboardEvent<T::ClipboardData> =
				match T::resolve_paste_data(mime_types, &mut data_access) {
					Ok(data) => ClipboardEvent::PasteResult { source, data },
					Err(error) => ClipboardEvent::FailedPasteHandling { source, error },
				};

			let _ = sender.send(event);
		});

		// This is to make the closure FnMut
		let mut handle = Some(handle);
		let _ = self
			.loop_handle
			.insert_source(channel, move |event, _, state| {
				if let calloop::channel::Event::Msg(event) = event {
					state.behaviour.callback(event);
					if let Some(handle) = handle.take() {
						let _ = handle.join();
					};
				}
			});
	}
}

impl<T: ClipboardConfig> SeatHandler for ClipboardHandler<T> {
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

impl<T: ClipboardConfig> DataDeviceHandler for ClipboardHandler<T> {
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

impl<T: ClipboardConfig> DataOfferHandler for ClipboardHandler<T> {
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

impl<T: ClipboardConfig> DataSourceHandler for ClipboardHandler<T> {
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

impl<T: ClipboardConfig> ProvidesRegistryState for ClipboardHandler<T> {
	registry_handlers![SeatState];

	fn registry(&mut self) -> &mut RegistryState {
		&mut self.registry_state
	}
}

impl<T: ClipboardConfig> Dispatch<WlKeyboard, ObjectId, ClipboardHandler<T>>
	for ClipboardHandler<T>
{
	fn event(
		state: &mut ClipboardHandler<T>,
		_proxy: &WlKeyboard,
		event: <WlKeyboard as Proxy>::Event,
		data: &ObjectId,
		_conn: &Connection,
		_qhandle: &sctk::reexports::client::QueueHandle<ClipboardHandler<T>>,
	) {
		match event {
			wl_keyboard::Event::Key { .. } | wl_keyboard::Event::Modifiers { .. } => {
				state.latest_seat = Some(data.clone());
			}
			_ => {}
		}
	}
}

impl<T: ClipboardConfig> TouchHandler for ClipboardHandler<T> {
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

impl<T: ClipboardConfig> PointerHandler for ClipboardHandler<T> {
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

delegate_seat!(@<T: ClipboardConfig> ClipboardHandler<T>);
delegate_touch!(@<T: ClipboardConfig> ClipboardHandler<T>);
delegate_pointer!(@<T: ClipboardConfig> ClipboardHandler<T>);
delegate_data_device!(@<T: ClipboardConfig> ClipboardHandler<T>);
delegate_registry!(@<T: ClipboardConfig> ClipboardHandler<T>);
