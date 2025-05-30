use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
	BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits, GetObjectW,
	HBITMAP, RGBQUAD, ReleaseDC,
};
use windows::Win32::System::DataExchange::{
	CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
	IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
};
use windows::Win32::System::Memory::*;
use windows::core::PCSTR;

use crate::Image;
use crate::{ClipboardData, ClipboardError, ClipboardEvent, InternalClipboard};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const GIF87A: &[u8] = b"GIF87a";
const GIF89A: &[u8] = b"GIF89a";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF";
const WEBP_MAGIC: &[u8] = b"RIFF";
const WEBP_SIGNATURE: &[u8] = b"WEBP";
const ICO_MAGIC: &[u8] = b"\x00\x00\x01\x00";
const TIFF_MAGIC_LE: &[u8] = b"II*\x00";
const TIFF_MAGIC_BE: &[u8] = b"MM\x00*";

const N_RETRIES: usize = 5;
const TIME_BETWEEN_RETRIES: Duration = Duration::from_millis(100);

pub fn try_read_clipboard_format(format: u32) -> Result<Vec<u8>, ClipboardError> {
	unsafe {
		let handle = GetClipboardData(format).map_err(|_| ClipboardError::Empty)?;
		if handle.0.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::Empty);
		}
		let hglobal = HGLOBAL(handle.0 as *mut _);
		let ptr = GlobalLock(hglobal) as *const u8;
		if ptr.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::LockFailed);
		}
		let size = GlobalSize(hglobal);
		let slice = std::slice::from_raw_parts(ptr, size);
		let data = slice.to_vec();
		let _ = GlobalUnlock(hglobal);
		Ok(data)
	}
}

pub fn try_read_clipboard_text() -> Result<String, ClipboardError> {
	unsafe {
		if OpenClipboard(None).is_err() {
			return Err(ClipboardError::OpenFailed);
		}
		IsClipboardFormatAvailable(CF_UNICODETEXT)
			.map_err(|_| ClipboardError::FormatNotAvailable)?;
		let handle = GetClipboardData(CF_UNICODETEXT).map_err(|_| ClipboardError::Empty)?;
		if handle.0.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::Empty);
		}
		let hglobal = HGLOBAL(handle.0);
		let ptr = GlobalLock(hglobal) as *const u16;
		if ptr.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::LockFailed);
		}
		let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
		let slice = std::slice::from_raw_parts(ptr, len);
		let result = String::from_utf16(slice).map_err(|_| ClipboardError::Utf16ConversionFailed);
		let _ = CloseClipboard();
		let _ = GlobalUnlock(hglobal);
		result
	}
}

pub fn try_read_clipboard_image() -> Result<Vec<u8>, ClipboardError> {
	unsafe {
		if OpenClipboard(None).is_err() {
			return Err(ClipboardError::OpenFailed);
		}
		IsClipboardFormatAvailable(CF_BITMAP).map_err(|_| ClipboardError::FormatNotAvailable)?;
		let handle = GetClipboardData(CF_BITMAP).map_err(|_| ClipboardError::Empty)?;
		if handle.0.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::Empty);
		}
		let hbitmap = HBITMAP(handle.0);
		let mut bmp = BITMAP::default();
		if GetObjectW(
			hbitmap.into(),
			std::mem::size_of::<BITMAP>() as i32,
			Some(&mut bmp as *mut _ as *mut _),
		) == 0
		{
			let _ = CloseClipboard();
			return Err(ClipboardError::LockFailed);
		}
		let mut bmi = BITMAPINFO {
			bmiHeader: BITMAPINFOHEADER {
				biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
				biWidth: bmp.bmWidth,
				biHeight: bmp.bmHeight,
				biPlanes: 1,
				biBitCount: 32,
				biCompression: BI_RGB.0,
				..Default::default()
			},
			bmiColors: [RGBQUAD::default(); 1],
		};
		let width = bmp.bmWidth as usize;
		let height = bmp.bmHeight as usize;
		let mut buffer = vec![0u8; width * height * 4];
		let hdc = GetDC(None);
		if hdc.0.is_null() {
			let _ = CloseClipboard();
			return Err(ClipboardError::LockFailed);
		}
		let scanlines = GetDIBits(
			hdc,
			hbitmap,
			0,
			height as u32,
			Some(buffer.as_mut_ptr() as *mut _),
			&mut bmi,
			DIB_RGB_COLORS,
		);
		ReleaseDC(None, hdc);
		let _ = CloseClipboard();
		if scanlines == 0 {
			return Err(ClipboardError::LockFailed);
		}
		Ok(buffer)
	}
}

fn detect_image_type(data: &[u8]) -> Option<Image> {
	if data.starts_with(GIF87A) || data.starts_with(GIF89A) {
		Some(Image::GIF(data.to_vec()))
	} else if data.starts_with(PNG_MAGIC) {
		Some(Image::PNG(data.to_vec()))
	} else if data.starts_with(JPEG_MAGIC) {
		Some(Image::JPEG(data.to_vec()))
	} else if data.len() > 12 && &data[0..4] == WEBP_MAGIC && &data[8..12] == WEBP_SIGNATURE {
		Some(Image::WEBP(data.to_vec()))
	} else if data.starts_with(ICO_MAGIC) {
		Some(Image::ICO(data.to_vec()))
	} else if data.starts_with(TIFF_MAGIC_LE) || data.starts_with(TIFF_MAGIC_BE) {
		Some(Image::TIFF(data.to_vec()))
	} else {
		None
	}
}

enum ThreadCommand {
	GetData,
	Write(ClipboardData),
	Exit,
}

pub struct WindowsClipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl InternalClipboard for WindowsClipboard {
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(callback: F) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(spawn_thread(receiver, callback));
		Self {
			sender,
			join_handle,
		}
	}

	fn get_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}

	fn write(&self, data: ClipboardData) {
		let _ = self.sender.send(ThreadCommand::Write(data));
	}
}

fn spawn_thread<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(
	receiver: Receiver<ThreadCommand>,
	mut callback: F,
) -> JoinHandle<()> {
	#[allow(clippy::manual_c_str_literals)]
	thread::spawn(move || {
		let png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr())) };
		let jpeg_format = unsafe { RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr())) };
		let image_png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr())) };
		let webp_format = unsafe { RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr())) };
		let html_format = unsafe { RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr())) };
		for command in receiver {
			match command {
				ThreadCommand::GetData => {
					let mut clipboard_opened = false;
					for _ in 0..N_RETRIES {
						unsafe {
							if OpenClipboard(None).is_ok() {
								clipboard_opened = true;
								break;
							}
						}
						thread::sleep(TIME_BETWEEN_RETRIES);
					}
					if !clipboard_opened {
						callback(ClipboardEvent::FailedPasteHandling(ClipboardError::InUse));
						continue;
					}

					unsafe {
						let mut format = 0u32;
						let mut found = false;
						loop {
							format = EnumClipboardFormats(format);
							if format == 0 {
								break;
							}
							match format {
								f if f == html_format => {
									if let Ok(data) = try_read_clipboard_format(f) {
										if let Ok(html) = String::from_utf8(data) {
											callback(ClipboardEvent::Paste(
												ClipboardData::Text(crate::Text::HTML(html)),
												None,
											));
											found = true;
											break;
										}
									}
								}
								f if f == CF_UNICODETEXT => {
									if let Ok(text) = try_read_clipboard_text() {
										callback(ClipboardEvent::Paste(
											ClipboardData::Text(crate::Text::Plain(text)),
											None,
										));
										found = true;
										break;
									}
								}
								f if f == png_format
									|| f == image_png_format || f == jpeg_format
									|| f == webp_format =>
								{
									if let Ok(data) = try_read_clipboard_format(f) {
										if let Some(image) = detect_image_type(&data) {
											callback(ClipboardEvent::Paste(
												ClipboardData::Image(image),
												None,
											));
											found = true;
											break;
										}
									}
								}
								f if f == CF_BITMAP => {
									if let Ok(data) = try_read_clipboard_image() {
										callback(ClipboardEvent::Paste(
											ClipboardData::Image(crate::Image::BMP(data)),
											None,
										));
										found = true;
										break;
									}
								}
								_ => {}
							}
							let _ = CloseClipboard();
						}
						if !found {
							callback(ClipboardEvent::FailedPasteHandling(
								ClipboardError::FormatNotAvailable,
							));
						}
					}
				}
				ThreadCommand::Write(data) => {
					let mut clipboard_opened = false;
					for _ in 0..N_RETRIES {
						unsafe {
							if OpenClipboard(None).is_ok() {
								clipboard_opened = true;
								break;
							}
						}
						thread::sleep(TIME_BETWEEN_RETRIES);
					}
					if !clipboard_opened {
						callback(ClipboardEvent::FailedPasteHandling(ClipboardError::InUse));
						continue;
					}

					unsafe {
						let _ = EmptyClipboard();
						match data {
							ClipboardData::Text(crate::Text::Plain(ref text)) => {
								if let Ok(wide) = widestring::U16CString::from_str(text) {
									let bytes = wide.as_slice_with_nul();
									let size = std::mem::size_of_val(bytes);
									let hmem_result = GlobalAlloc(GMEM_MOVEABLE, size);
									if let Ok(hmem) = hmem_result {
										if !hmem.0.is_null() {
											let ptr = GlobalLock(hmem) as *mut u8;
											if !ptr.is_null() {
												std::ptr::copy_nonoverlapping(
													bytes.as_ptr() as *const u8,
													ptr,
													size,
												);
												let _ = GlobalUnlock(hmem);
												let _ = SetClipboardData(
													CF_UNICODETEXT,
													Some(HANDLE(hmem.0)),
												);
											}
										}
									}
								}
							}
							ClipboardData::Text(crate::Text::HTML(ref html)) => {
								let html_bytes = html.as_bytes();
								let size = html_bytes.len();
								let hmem_result = GlobalAlloc(GMEM_MOVEABLE, size);
								if let Ok(hmem) = hmem_result {
									if !hmem.0.is_null() {
										let ptr = GlobalLock(hmem) as *mut u8;
										if !ptr.is_null() {
											std::ptr::copy_nonoverlapping(
												html_bytes.as_ptr(),
												ptr,
												size,
											);
											let _ = GlobalUnlock(hmem);
											let _ =
												SetClipboardData(html_format, Some(HANDLE(hmem.0)));
										}
									}
								}
							}
							ClipboardData::Image(ref image) => {
								// Image conversions here
								if let crate::Image::PNG(data) = image {
									let size = data.len();
									let hmem_result = GlobalAlloc(GMEM_MOVEABLE, size);
									if let Ok(hmem) = hmem_result {
										if !hmem.0.is_null() {
											let ptr = GlobalLock(hmem) as *mut u8;
											if !ptr.is_null() {
												std::ptr::copy_nonoverlapping(
													data.as_ptr(),
													ptr,
													size,
												);
												let _ = GlobalUnlock(hmem);
												let _ = SetClipboardData(
													png_format,
													Some(HANDLE(hmem.0)),
												);
											}
										}
									}
								}
							}
						}
						let _ = CloseClipboard();
					}
				}
				ThreadCommand::Exit => {
					return;
				}
			}
		}
	})
}

impl Drop for WindowsClipboard {
	fn drop(&mut self) {
		if let Ok(()) = self.sender.send(ThreadCommand::Exit) {
			let _ = self.join_handle.take().unwrap().join();
		}
	}
}
