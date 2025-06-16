use crate::InternalClipboard;

pub struct WasmClipboard {}

impl InternalClipboard for WasmClipboard {
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(callback: F) -> Self {
		todo!()
	}

	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
