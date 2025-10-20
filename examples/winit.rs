use std::{borrow::Cow, num::NonZeroU32, rc::Rc, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use log::warn;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use gengine_clipboard::{Clipboard, ClipboardConfig, ClipboardEvent};
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

struct ExampleConfig {
	proxy: EventLoopProxy<ClipboardEvent<ClipboardData>>,
}

#[derive(Debug)]
enum ClipboardData {
	Text(String),
	Png(Vec<u8>),
}

impl ClipboardConfig for ExampleConfig {
	type ClipboardData = ClipboardData;

	fn callback(&mut self, event: ClipboardEvent<ClipboardData>) {
		let _ = self.proxy.send_event(event);
	}

	fn resolve_paste_data(
		mime_types: Vec<String>,
		data_access: &mut impl gengine_clipboard::PasteDataAccess,
	) -> Result<Self::ClipboardData, gengine_clipboard::ClipboardError> {
		warn!("Got mime types: {:?}", mime_types);

		let result = if mime_types.contains(&String::from("image/png")) {
			data_access.get_data("image/png")
		} else if mime_types.contains(&String::from("PNG")) {
			data_access.get_data("PNG")
		} else if mime_types.contains(&String::from("text/plain;charset=utf-8")) {
			data_access.get_data("text/plain;charset=utf-8")
		} else if mime_types.contains(&String::from("UTF8_STRING")) {
			data_access.get_data("UTF8_STRING")
		} else if mime_types.contains(&String::from("text/plain")) {
			data_access.get_data("text/plain")
		} else if mime_types.contains(&String::from("CF_UNICODETEXT")) {
			data_access.get_data("CF_UNICODETEXT").map(|data| {
				let data: Vec<u16> = data
					.chunks(2)
					.map(|v| ((v[1] as u16) << 8) | v[0] as u16)
					.collect();
				String::from_utf16_lossy(&data).as_bytes().to_vec()
			})
		} else {
			return Err(gengine_clipboard::ClipboardError::UnsupportedMimeType);
		};

		match result {
			Ok(data) => {
				if mime_types.contains(&String::from("image/png"))
					|| mime_types.contains(&String::from("PNG"))
				{
					Ok(ClipboardData::Png(data))
				} else if let Cow::Owned(string) = String::from_utf8_lossy(&data) {
					Ok(ClipboardData::Text(string))
				} else {
					// Not owned means that it is valid.
					Ok(ClipboardData::Text(String::from_utf8(data).unwrap()))
				}
			}
			Err(error) => Err(error),
		}
	}
}

struct ExampleWindow {
	proxy: EventLoopProxy<ClipboardEvent<ClipboardData>>,
	context_surface: Option<ContextSurface>,
	clipboard: Option<Clipboard<ExampleConfig>>,
	ctrl_left: bool,
	ctrl_right: bool,
}

impl ApplicationHandler<ClipboardEvent<ClipboardData>> for ExampleWindow {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.context_surface.is_none() {
			let window_attributes = Window::default_attributes().with_title("Clipboard Example");
			let window = Rc::new(event_loop.create_window(window_attributes).unwrap());

			let proxy = self.proxy.clone();
			self.clipboard = Some(Clipboard::new(&window, ExampleConfig { proxy }));

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

	fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ClipboardEvent<ClipboardData>) {
		match event {
			ClipboardEvent::StartedPasteHandling { source } => {
				warn!("Started paste handling {:?}", source);
			}
			ClipboardEvent::FailedPasteHandling { source, error } => {
				warn!("Failed paste handling {:?} with error {:?}", source, error)
			}
			ClipboardEvent::PasteResult { source, data } => match data {
				ClipboardData::Text(text) => {
					warn!("Reseived from {:?} the string: {}", source, text);
				}
				ClipboardData::Png(png) => {
					warn!(
						"Received a PNG from {:?}. Saving it into image.png.",
						source
					);
					std::fs::write("./image.png", png).expect("Failed to write into file")
				}
			},
		}
	}
}

fn main() {
	#[cfg(target_arch = "wasm32")]
	{
		console_error_panic_hook::set_once();
		let log_config = wasm_logger::Config::new(log::Level::Debug);
		wasm_logger::init(log_config);
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		env_logger::builder()
			.filter_level(log::LevelFilter::Debug)
			.init();
	}
	let event_loop = EventLoop::<ClipboardEvent<ClipboardData>>::with_user_event()
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
