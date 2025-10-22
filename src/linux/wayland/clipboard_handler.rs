use sctk::{
	data_device_manager::{
		DataDeviceManagerState,
		data_device::{DataDevice, DataDeviceHandler},
		data_offer::DataOfferHandler,
		data_source::DataSourceHandler,
	},
	delegate_data_device, delegate_pointer, delegate_registry, delegate_seat, delegate_touch,
	reexports::{
		calloop::LoopHandle,
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
use std::collections::HashMap;

use crate::{
	ClipboardError, ClipboardEventSource, ClipboardHandler,
	platform::wayland::{
		even_handler_thread::HandlerThread, paste_data_access::WaylandPasteDataAccess,
	},
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

pub struct WaylandHandler {
	registry_state: RegistryState,
	seat_state: SeatState,
	data_device_manager_state: Option<DataDeviceManagerState>,
	seats: HashMap<ObjectId, SeatCapabilities>,
	latest_seat: Option<ObjectId>,
	even_count: usize,
	handler: HandlerThread,
	pub exit: bool,
}

impl WaylandHandler {
	pub fn create_and_insert<T: ClipboardHandler>(
		backend: Backend,
		loop_handle: LoopHandle<'static, Self>,
		handler: T,
	) -> Self {
		let connection = Connection::from_backend(backend);
		let (globals, event_queue) = registry_queue_init::<WaylandHandler>(&connection).unwrap();
		let queue_handle = &event_queue.handle();

		let data_device_manager_state = DataDeviceManagerState::bind(&globals, queue_handle).ok();
		let seat_state = SeatState::new(&globals, queue_handle);

		#[allow(clippy::mutable_key_type)]
		let mut seats = HashMap::new();
		for seat in seat_state.seats() {
			seats.insert(seat.id(), Default::default());
		}

		WaylandSource::new(connection, event_queue)
			.insert(loop_handle)
			.unwrap();

		let handler = HandlerThread::new(handler);

		Self {
			registry_state: RegistryState::new(&globals),
			seat_state,
			data_device_manager_state,
			seats,
			latest_seat: None,
			exit: false,
			handler,
			even_count: 0,
		}
	}

	pub fn request_data(&mut self) {
		let source = ClipboardEventSource {
			value: self.even_count,
		};
		self.even_count += 1;

		self.handler.started_paste_handling(source);

		let latest = match self.latest_seat.as_ref() {
			Some(latest) => latest,
			_ => {
				self.handler.failed_paste_handling(
					ClipboardError::Unknown("No latest in wayland".to_string()),
					source,
				);
				return;
			}
		};

		let seat = match self.seats.get_mut(latest) {
			Some(seat) => seat,
			_ => {
				self.handler.failed_paste_handling(
					ClipboardError::Unknown("Latest seat not available in wayland".to_string()),
					source,
				);
				return;
			}
		};

		let selection = match seat.data_device.as_ref() {
			Some(data_device) => match data_device.data().selection_offer() {
				Some(selection) => selection,
				_ => {
					self.handler
						.failed_paste_handling(ClipboardError::Empty, source);
					return;
				}
			},
			_ => {
				self.handler.failed_paste_handling(
					ClipboardError::Unknown("no data device in wayland".to_string()),
					source,
				);
				return;
			}
		};

		let data = WaylandPasteDataAccess::new(selection);
		self.handler.paste_result(data, source);
	}
}

impl SeatHandler for WaylandHandler {
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

impl DataDeviceHandler for WaylandHandler {
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

impl DataOfferHandler for WaylandHandler {
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

impl DataSourceHandler for WaylandHandler {
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

impl ProvidesRegistryState for WaylandHandler {
	registry_handlers![SeatState];

	fn registry(&mut self) -> &mut RegistryState {
		&mut self.registry_state
	}
}

impl Dispatch<WlKeyboard, ObjectId, WaylandHandler> for WaylandHandler {
	fn event(
		state: &mut WaylandHandler,
		_proxy: &WlKeyboard,
		event: <WlKeyboard as Proxy>::Event,
		data: &ObjectId,
		_conn: &Connection,
		_qhandle: &sctk::reexports::client::QueueHandle<WaylandHandler>,
	) {
		match event {
			wl_keyboard::Event::Key { .. } | wl_keyboard::Event::Modifiers { .. } => {
				state.latest_seat = Some(data.clone());
			}
			_ => {}
		}
	}
}

impl TouchHandler for WaylandHandler {
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

impl PointerHandler for WaylandHandler {
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

delegate_seat!(WaylandHandler);
delegate_touch!(WaylandHandler);
delegate_pointer!(WaylandHandler);
delegate_data_device!(WaylandHandler);
delegate_registry!(WaylandHandler);
