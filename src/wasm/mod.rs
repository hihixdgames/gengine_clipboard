mod chain;
mod pasta_data_access;

use js_sys::{Array, Function, Uint8Array};
use raw_window_handle::HasDisplayHandle;
use std::marker::PhantomData;
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use web_sys::console;

use crate::{
	ClipboardConfig, ClipboardError, ClipboardEvent, ClipboardEventSource, InternalClipboard,
	platform::{chain::start, pasta_data_access::WasmDataAccess},
};

pub struct Clipboard<T: ClipboardConfig> {
	_callback: Closure<dyn FnMut(JsValue, JsValue)>,
	_on_paste: Closure<dyn FnMut(web_sys::ClipboardEvent)>,
	phantom: PhantomData<T>,
}

fn console_log(text: &str) {
	let js_value = JsValue::from_str(text);
	console::log(&Array::of1(&js_value));
}

impl<T: ClipboardConfig> InternalClipboard<T> for Clipboard<T> {
	fn new(_display_handle: &dyn HasDisplayHandle, mut config: T) -> Self {
		console_log("hello");

		let callback = Closure::<dyn FnMut(_, _)>::new(move |data: JsValue, source: JsValue| {
			let source = source.as_f64().unwrap();
			let source = ClipboardEventSource {
				value: source as usize,
			};

			let mut cleaned = Vec::new();
			let data: Array = data.dyn_into().unwrap();
			let len = data.length() / 2;
			for i in 0..len {
				let name = data.get(i).as_string().unwrap();
				let array: Uint8Array = data.get(i + 1).dyn_into().unwrap();
				cleaned.push((name, array));
			}

			if cleaned.is_empty() {
				config.callback(ClipboardEvent::FailedPasteHandling {
					source,
					error: ClipboardError::Empty,
				});
			} else {
				let mut mime_types = Vec::new();
				for (mime, _) in cleaned.iter() {
					mime_types.push(mime.clone());
				}

				let mut data_access = WasmDataAccess::new(cleaned);
				let event: ClipboardEvent<T::ClipboardData> =
					match T::resolve_paste_data(mime_types, &mut data_access) {
						Ok(data) => ClipboardEvent::PasteResult { source, data },
						Err(error) => ClipboardEvent::FailedPasteHandling { source, error },
					};

				config.callback(event);
			}
		});

		let mut n_events = 0;
		let function: &Function = callback.as_ref().unchecked_ref();
		let function = function.clone();
		let on_paste = Closure::<dyn FnMut(_)>::new(move |event: web_sys::ClipboardEvent| {
			let data = event.clipboard_data().unwrap();
			let items = data.items();
			let n_items = items.length();
			for i in (0..n_items).rev() {
				let item = items.get(i).unwrap();
				let ty = item.type_();
				let a = "Type: ".to_owned() + &ty;
				console_log(&a);
			}

			start(data, n_events, function.clone());
			n_events += 1;
		});

		let window = web_sys::window().unwrap();
		let document = window.document().unwrap();
		let _ =
			document.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref());

		Self {
			_callback: callback,
			_on_paste: on_paste,
			phantom: PhantomData,
		}
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
