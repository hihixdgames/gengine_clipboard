use std::{num::NonZeroU32, rc::Rc, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use gengine_clipboard::{Clipboard, ClipboardEvent, ClipboardHandler};
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
	proxy: EventLoopProxy<ClipboardData>,
}

#[derive(Debug)]
enum ClipboardData {
	Text(String),
	Png(Vec<u8>),
}

impl ClipboardHandler for ExampleConfig {
	fn handle_event(&mut self, event: ClipboardEvent<'_>) {
		match event {
			ClipboardEvent::StartedPasteHandling { source } => {
				log::info!("Started paste handling {source:?}");
			}
			ClipboardEvent::FailedPasteHandling { source, error } => {
				log::error!("Failed paste handling {source:?} with error {error:?}")
			}
			ClipboardEvent::PasteResult { source, data } => {
				log::info!("Got mime types: {:?} from {source:?}", data.raw_types());

				if let Some(bytes) = data.get_first_success(&["image/png", "PNG"]) {
					let _ = self.proxy.send_event(ClipboardData::Png(bytes));
				} else if let Some(string) = data.read_data::<String>() {
					let _ = self.proxy.send_event(ClipboardData::Text(string));
				} else {
					log::error!("Could not get wanted data for {source:?}");
				};
			}
		}
	}
}

struct ExampleWindow {
	proxy: EventLoopProxy<ClipboardData>,
	context_surface: Option<ContextSurface>,
	clipboard: Option<Clipboard>,
	ctrl_left: bool,
	ctrl_right: bool,
}

impl ApplicationHandler<ClipboardData> for ExampleWindow {
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

	fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ClipboardData) {
		match event {
			ClipboardData::Text(text) => {
				log::info!("Reseived from the string: {}", text);
			}
			ClipboardData::Png(png) => {
				#[cfg(not(target_arch = "wasm32"))]
				{
					log::info!("Received a PNG from. Saving it into image.png.");
					std::fs::write("./image.png", png).expect("Failed to write into file")
				}
				#[cfg(target_arch = "wasm32")]
				{
					use js_sys::Uint8Array;
					use wasm_bindgen::JsCast;
					use web_sys::{Blob, Url, window};

					log::info!("Received a PNG from. Adding it to the document.");

					let uint8_array = Uint8Array::new_from_slice(&png);
					let array = js_sys::Array::new();
					array.push(&uint8_array);
					let blob = Blob::new_with_u8_array_sequence(&array).unwrap();
					let url = Url::create_object_url_with_blob(&blob).unwrap();

					let window = window().unwrap();
					let document = window.document().unwrap();

					let image: web_sys::HtmlImageElement =
						document.create_element("img").unwrap().dyn_into().unwrap();
					image.set_src(&url);

					let body = document.body().unwrap();
					body.append_child(&image.into()).unwrap();
				}
			}
		}
	}
}

fn main() {
	#[cfg(target_arch = "wasm32")]
	{
		console_error_panic_hook::set_once();
		let log_config = wasm_logger::Config::new(log::Level::Info);
		wasm_logger::init(log_config);
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		env_logger::builder()
			.filter_level(log::LevelFilter::Info)
			.init();
	}
	let event_loop = EventLoop::<ClipboardData>::with_user_event()
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
