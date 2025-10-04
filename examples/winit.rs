use std::{
	num::NonZeroU32,
	rc::Rc,
	time::{Duration, Instant},
};

use gengine_clipboard::{Clipboard, ClipboardEvent};
use softbuffer::{Context, Surface};
use winit::{
	application::ApplicationHandler,
	event::WindowEvent,
	event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
	keyboard::{KeyCode, PhysicalKey},
	window::{Window, WindowId},
};

struct ContextSurface {
	_context: Context<Rc<Window>>,
	_surface: Surface<Rc<Window>, Rc<Window>>,
}

struct ExampleWindow {
	proxy: EventLoopProxy<ClipboardEvent>,
	context_surface: Option<ContextSurface>,
	clipboard: Option<Clipboard>,
	ctrl_left: bool,
	ctrl_right: bool,
}

impl ApplicationHandler<ClipboardEvent> for ExampleWindow {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.context_surface.is_none() {
			let window_attributes = Window::default_attributes().with_title("Clipboard Example");
			let window = Rc::new(event_loop.create_window(window_attributes).unwrap());

			let proxy = self.proxy.clone();
			self.clipboard = Some(Clipboard::new(&window, move |event: ClipboardEvent| {
				let _ = proxy.send_event(event);
			}));

			let context = Context::new(window.clone()).unwrap();
			let mut surface = Surface::new(&context, window.clone()).unwrap();

			let (width, height) = {
				let size = window.inner_size();
				(size.width.max(1), size.height.max(1))
			};

			surface
				.resize(
					NonZeroU32::new(width).unwrap(),
					NonZeroU32::new(height).unwrap(),
				)
				.unwrap();

			let buffer = surface.buffer_mut().unwrap();
			buffer.present().unwrap();

			self.context_surface = Some(ContextSurface {
				_context: context,
				_surface: surface,
			});
		}
	}

	fn window_event(
		&mut self,
		_event_loop: &ActiveEventLoop,
		_window_id: WindowId,
		event: WindowEvent,
	) {
		if let WindowEvent::KeyboardInput { event, .. } = event {
			match event.physical_key {
				PhysicalKey::Code(KeyCode::ControlLeft) => {
					self.ctrl_left = event.state.is_pressed();
				}
				PhysicalKey::Code(KeyCode::ControlRight) => {
					self.ctrl_right = event.state.is_pressed();
				}
				#[cfg(not(target_arch = "wasm32"))]
				PhysicalKey::Code(KeyCode::KeyV) => {
					if self.clipboard.is_none() {
						return;
					}
					if (self.ctrl_left || self.ctrl_right) && event.state.is_pressed() {
						self.clipboard.as_ref().unwrap().request_data();
					}
				}
				_ => (),
			}
		}
	}

	fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ClipboardEvent) {
		println!("Got clipboard event: {event:?}");
	}
}

fn main() {
	let event_loop = EventLoop::<ClipboardEvent>::with_user_event()
		.build()
		.unwrap();

	let mut example_window = ExampleWindow {
		clipboard: None,
		context_surface: None,
		proxy: event_loop.create_proxy(),
		ctrl_left: false,
		ctrl_right: false,
	};

	event_loop.set_control_flow(ControlFlow::WaitUntil(
		Instant::now() + Duration::from_millis(2),
	));

	let _ = event_loop.run_app(&mut example_window);
}
