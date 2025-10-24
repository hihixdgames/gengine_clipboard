mod collector;
mod pasta_data_access;

use js_sys::Uint8Array;
use raw_window_handle::HasDisplayHandle;
use wasm_bindgen::{JsCast, prelude::Closure};
use web_sys::{Event, FileReader};

use crate::{
	ClipboardError, ClipboardHandler, InternalClipboard,
	platform::collector::{Collector, CollectorHandle},
};

pub use pasta_data_access::WasmDataAccess as DataAccess;

pub struct Clipboard {
	_handle: CollectorHandle,
}

impl InternalClipboard for Clipboard {
	fn new<T: ClipboardHandler>(_display_handle: &dyn HasDisplayHandle, handler: T) -> Self {
		let (handle, collector) = Collector::new(handler);

		let mut n_events = 0;

		let inner_collector = collector.clone();
		let on_paste = Closure::<dyn FnMut(_)>::new(move |event: web_sys::ClipboardEvent| {
			let data = event.clipboard_data().unwrap();
			let items = data.items();

			collector.start_paste_handling(items.length() as usize, n_events);

			if items.length() == 0 {
				return collector.send_error(ClipboardError::Empty, n_events);
			}

			for i in 0..items.length() {
				let item = items.get(i).unwrap();

				let mime_type = item.type_();
				match item.kind().as_str() {
					"string" => {
						let collector = inner_collector.clone();
						let mime_type = mime_type.clone();
						let callback = Closure::once_into_js(move |event: Event| {
							let string = event.as_string().unwrap();
							let array = Uint8Array::new_from_slice(string.as_bytes());
							collector.insert_data(mime_type, array, n_events);
						});

						let _ = item.get_as_string(Some(callback.as_ref().unchecked_ref()));
					}
					"file" => {
						let file = item.get_as_file().unwrap().unwrap();
						let file_reader = FileReader::new().unwrap();
						file_reader.read_as_array_buffer(&file).unwrap();

						let collector = inner_collector.clone();
						let mime_type = mime_type.clone();
						let onload = Closure::once_into_js(move |event: Event| {
							let file_reader: FileReader =
								event.target().unwrap().dyn_into().unwrap();
							let file = file_reader.result().unwrap();
							let array = js_sys::Uint8Array::new(&file);
							collector.insert_data(mime_type, array, n_events);
						});

						file_reader.set_onload(Some(onload.as_ref().unchecked_ref()));
					}
					_ => unreachable!(),
				}
			}

			n_events += 1;
		});

		let window = web_sys::window().unwrap();
		let document = window.document().unwrap();
		let _ =
			document.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref());
		on_paste.forget();

		Self { _handle: handle }
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
