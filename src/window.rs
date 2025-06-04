use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
	BI_RGB, BITMAP, BITMAPFILEHEADER, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC,
	GetDIBits, GetObjectW, HBITMAP, RGBQUAD, ReleaseDC,
};
use windows::Win32::System::DataExchange::{
	CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
	IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
};
use windows::Win32::System::Memory::*;
use windows::core::PCSTR;

use crate::clipboard_data::{ClipboardData, Image, Text};
use crate::{ClipboardError, ClipboardEvent, InternalClipboard};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const BF_TYPE_BM: u16 = 0x4D42;
const CF_HDROP: u32 = 15;

const N_RETRIES: usize = 5;
const TIME_BETWEEN_RETRIES: u64 = 100;

impl fmt::Display for Text {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Text::Plain(s) | Text::HTML(s) => write!(f, "{}", s),
		}
	}
}

fn retry_or_err(
	attempt: usize,
	max_retries: usize,
	err: ClipboardError,
) -> Result<(), ClipboardError> {
	if attempt == max_retries {
		Err(err)
	} else {
		std::thread::sleep(std::time::Duration::from_millis(TIME_BETWEEN_RETRIES));
		unsafe {
			OpenClipboard(None).map_err(|_| ClipboardError::OpenFailed)?;
		}
		Ok(())
	}
}

pub fn try_read_clipboard_bitmap_and_save<P: AsRef<std::path::Path>>(
	output_path: P,
) -> Result<(), ClipboardError> {
	unsafe {
		let handle = GetClipboardData(CF_BITMAP).map_err(|_| ClipboardError::Empty)?;
		if handle.0.is_null() {
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
		let pixel_data_size = width * height * 4;
		let mut buffer = vec![0u8; pixel_data_size];

		let hdc = GetDC(None);
		if hdc.0.is_null() {
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

		if scanlines == 0 {
			return Err(ClipboardError::LockFailed);
		}

		let file_header_size = std::mem::size_of::<BITMAPFILEHEADER>();
		let info_header_size = std::mem::size_of::<BITMAPINFOHEADER>();
		let file_size = file_header_size + info_header_size + pixel_data_size;

		let file_header = BITMAPFILEHEADER {
			bfType: BF_TYPE_BM,
			bfSize: file_size as u32,
			bfReserved1: 0,
			bfReserved2: 0,
			bfOffBits: (file_header_size + info_header_size) as u32,
		};

		let mut file = File::create(output_path).map_err(|_| ClipboardError::WriteFailed)?;

		file.write_all(std::slice::from_raw_parts(
			&file_header as *const _ as *const u8,
			file_header_size,
		))
		.map_err(|_| ClipboardError::WriteFailed)?;

		file.write_all(std::slice::from_raw_parts(
			&bmi.bmiHeader as *const _ as *const u8,
			info_header_size,
		))
		.map_err(|_| ClipboardError::WriteFailed)?;

		file.write_all(&buffer)
			.map_err(|_| ClipboardError::WriteFailed)?;

		Ok(())
	}
}

pub fn detect_image_type(data: &[u8]) -> Option<Image> {
	if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
		Some(Image::GIF(data.to_vec()))
	} else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
		Some(Image::PNG(data.to_vec()))
	} else if data.starts_with(b"\xFF\xD8\xFF") {
		Some(Image::JPEG(data.to_vec()))
	} else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
		Some(Image::WEBP(data.to_vec()))
	} else {
		None
	}
}

pub struct ClipboardHandle;

impl ClipboardHandle {
	pub fn new() -> Result<Self, ClipboardError> {
		unsafe {
			OpenClipboard(None).map_err(|_| ClipboardError::OpenFailed)?;
		}
		Ok(Self {})
	}

	pub fn read_data(&self, format: u32) -> Result<ClipboardData, ClipboardError> {
		unsafe {
			let png_format = RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr()));
			let jpeg_format = RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr()));
			let image_png_format = RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr()));
			let image_jpeg_format = RegisterClipboardFormatA(PCSTR(b"image/jpeg\0".as_ptr()));
			let gif_format = RegisterClipboardFormatA(PCSTR(b"GIF\0".as_ptr()));
			let webp_format = RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr()));
			let html_format = RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr()));

			for attempt in 1..=N_RETRIES {
				if IsClipboardFormatAvailable(format).is_err() {
					retry_or_err(attempt, N_RETRIES, ClipboardError::FormatNotAvailable)?;
					continue;
				}

				let handle_result = GetClipboardData(format);
				if handle_result.is_err() || handle_result.as_ref().unwrap().0.is_null() {
					retry_or_err(attempt, N_RETRIES, ClipboardError::Empty)?;
					continue;
				}
				let handle = handle_result.unwrap();

				if format == CF_UNICODETEXT {
					let hglobal = HGLOBAL(handle.0);
					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						retry_or_err(attempt, N_RETRIES, ClipboardError::LockFailed)?;
						continue;
					}

					let len = (0..)
						.take_while(|&i| *(ptr.add(i * 2) as *const u16) != 0)
						.count();
					let slice = std::slice::from_raw_parts(ptr as *const u16, len);
					let string = String::from_utf16(slice)
						.map_err(|_| ClipboardError::Utf16ConversionFailed)?;

					let _ = GlobalUnlock(hglobal);
					return Ok(ClipboardData::Text(Text::Plain(string)));
				}

				if format == html_format {
					let hglobal = HGLOBAL(handle.0);
					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						retry_or_err(attempt, N_RETRIES, ClipboardError::LockFailed)?;
						continue;
					}

					let size = GlobalSize(hglobal);
					let slice = std::slice::from_raw_parts(ptr as *const u8, size);
					let html = String::from_utf8_lossy(slice).to_string();

					let _ = GlobalUnlock(hglobal);
					return Ok(ClipboardData::Text(Text::HTML(html)));
				}

				if format == CF_HDROP {
					use std::{ffi::OsString, os::windows::ffi::OsStringExt};
					use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

					let hglobal = HGLOBAL(handle.0);
					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						retry_or_err(attempt, N_RETRIES, ClipboardError::LockFailed)?;
						continue;
					}

					let hdrop = HDROP(ptr);
					let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);

					for i in 0..file_count {
						let mut buffer = vec![0u16; 260];
						let len = DragQueryFileW(hdrop, i, Some(&mut buffer[..]));
						buffer.truncate(len as usize);

						if let Ok(path) = OsString::from_wide(&buffer).into_string() {
							let path_obj = std::path::Path::new(&path);
							if let Some(ext) = path_obj.extension().and_then(|e| e.to_str()) {
								let ext_lower = ext.to_ascii_lowercase();

								let file_data = match std::fs::read(path_obj) {
									Ok(data) => data,
									Err(_) => continue,
								};

								let output_filename = match ext_lower.as_str() {
									"png" => "png.png",
									"jpeg" | "jpg" => "jpeg.jpg",
									"webp" => "webp.webp",
									"gif" => "gif.gif",
									_ => continue,
								};

								if std::fs::write(output_filename, &file_data).is_err() {
									continue;
								}

								if let Some(image) = detect_image_type(&file_data) {
									let _ = GlobalUnlock(hglobal);
									return Ok(ClipboardData::Image(image));
								}
							}
						}
					}

					let _ = GlobalUnlock(hglobal);
					retry_or_err(attempt, N_RETRIES, ClipboardError::FormatNotAvailable)?;
					continue;
				}

				if format == CF_BITMAP {
					let output_path = std::path::Path::new("bitmap.bmp");
					match try_read_clipboard_bitmap_and_save(output_path) {
						Ok(_) => {
							let data = std::fs::read(output_path)
								.map_err(|_| ClipboardError::ReadFailed)?;
							return Ok(ClipboardData::Image(Image::BMP(data)));
						}
						Err(_) => {
							retry_or_err(attempt, N_RETRIES, ClipboardError::WriteFailed)?;
							continue;
						}
					}
				}

				if format == png_format
					|| format == image_png_format
					|| format == jpeg_format
					|| format == image_jpeg_format
					|| format == gif_format
					|| format == webp_format
				{
					let hglobal = HGLOBAL(handle.0);
					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						retry_or_err(attempt, N_RETRIES, ClipboardError::LockFailed)?;
						continue;
					}

					let size = GlobalSize(hglobal);
					let slice = std::slice::from_raw_parts(ptr as *const u8, size);
					let image_data = slice.to_vec();

					let _ = GlobalUnlock(hglobal);

					let ext = match detect_image_type(&image_data) {
						Some(Image::PNG(_)) => "png",
						Some(Image::JPEG(_)) => "jpg",
						Some(Image::GIF(_)) => "gif",
						Some(Image::WEBP(_)) => "webp",
						_ => "img",
					};

					let filename = format!("clipboard.{}", ext);
					let output_path = std::path::Path::new(&filename);
					if std::fs::write(output_path, &image_data).is_err() {
						retry_or_err(attempt, N_RETRIES, ClipboardError::WriteFailed)?;
						continue;
					}

					let file_data =
						std::fs::read(output_path).map_err(|_| ClipboardError::ReadFailed)?;
					if let Some(image) = detect_image_type(&file_data) {
						return Ok(ClipboardData::Image(image));
					}

					retry_or_err(attempt, N_RETRIES, ClipboardError::FormatNotAvailable)?;
					continue;
				}

				retry_or_err(attempt, N_RETRIES, ClipboardError::FormatNotAvailable)?;
				continue;
			}
		}

		Err(ClipboardError::FormatNotAvailable)
	}

	pub fn write_data(&self, data: &ClipboardData) -> Result<(), ClipboardError> {
		unsafe {
			match data {
				ClipboardData::Text(text) => {
					let s = match text {
						Text::Plain(plain) => plain.as_str(),
						Text::HTML(html) => html.as_str(),
					};

					let wide: Vec<u16> = OsStr::new(s)
						.encode_wide()
						.chain(std::iter::once(0))
						.collect();

					let size = wide.len() * std::mem::size_of::<u16>();

					let hglobal = GlobalAlloc(GMEM_MOVEABLE, size)
						.map_err(|_| ClipboardError::AllocationFailed)?;

					if hglobal.is_invalid() {
						return Err(ClipboardError::AllocationFailed);
					}

					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						let _ = GlobalUnlock(hglobal);
						return Err(ClipboardError::LockFailed);
					}

					std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, size);
					let _ = GlobalUnlock(hglobal);

					SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hglobal.0)))
						.map_err(|_| ClipboardError::SetFailed)?;
				}

				ClipboardData::Image(image) => match image {
					Image::PNG(data) => {
						let png_format = RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr()));

						let hglobal = GlobalAlloc(GMEM_MOVEABLE, data.len())
							.map_err(|_| ClipboardError::AllocationFailed)?;

						if hglobal.is_invalid() {
							return Err(ClipboardError::AllocationFailed);
						}

						let ptr = GlobalLock(hglobal);
						if ptr.is_null() {
							let _ = GlobalUnlock(hglobal);
							return Err(ClipboardError::LockFailed);
						}

						std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
						let _ = GlobalUnlock(hglobal);

						SetClipboardData(png_format, Some(HANDLE(hglobal.0)))
							.map_err(|_| ClipboardError::SetFailed)?;
					}

					Image::BMP(data) => {
						let hglobal = GlobalAlloc(GMEM_MOVEABLE, data.len())
							.map_err(|_| ClipboardError::AllocationFailed)?;

						if hglobal.is_invalid() {
							return Err(ClipboardError::AllocationFailed);
						}

						let ptr = GlobalLock(hglobal);
						if ptr.is_null() {
							let _ = GlobalUnlock(hglobal);
							return Err(ClipboardError::LockFailed);
						}

						std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
						let _ = GlobalUnlock(hglobal);

						SetClipboardData(CF_DIB, Some(HANDLE(hglobal.0)))
							.map_err(|_| ClipboardError::SetFailed)?;
					}

					_ => return Err(ClipboardError::FormatNotAvailable),
				},
			}

			Ok(())
		}
	}
}

impl Drop for ClipboardHandle {
	fn drop(&mut self) {
		unsafe {
			CloseClipboard().ok();
		}
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
	thread::spawn(move || {
		for command in receiver {
			match command {
				ThreadCommand::GetData => {
					let handle = match ClipboardHandle::new() {
						Ok(h) => h,
						Err(e) => {
							callback(ClipboardEvent::FailedPasteHandling(e));
							continue;
						}
					};

					let mut format = 0u32;
					let mut found = false;

					loop {
						format = unsafe { EnumClipboardFormats(format) };
						if format == 0 {
							break;
						}

						if let Ok(data) = handle.read_data(format) {
							callback(ClipboardEvent::Paste(data, None));
							found = true;
							break;
						}
					}

					if !found {
						callback(ClipboardEvent::FailedPasteHandling(
							ClipboardError::FormatNotAvailable,
						));
					}
				}

				ThreadCommand::Write(data) => {
					let handle = match ClipboardHandle::new() {
						Ok(h) => h,
						Err(e) => {
							callback(ClipboardEvent::FailedPasteHandling(e));
							continue;
						}
					};

					if let Err(e) = handle.write_data(&data) {
						callback(ClipboardEvent::FailedPasteHandling(e));
					}
				}

				ThreadCommand::Exit => return,
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
