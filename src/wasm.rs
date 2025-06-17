use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use web_sys::console;

use crate::InternalClipboard;

pub struct WasmClipboard {}

impl InternalClipboard for WasmClipboard {
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(callback: F) -> Self {
		let closure = Closure::<dyn FnMut(_)>::new(move |event: web_sys::ClipboardEvent| {
			console::log_1(&JsValue::from_str("Hello World"));
		});

		let window = web_sys::window().unwrap();
		let document = window.document().unwrap();

		let _ =
			document.add_event_listener_with_callback("paste", closure.as_ref().unchecked_ref());
		closure.forget();

		Self {}
	}

	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
