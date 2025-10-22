#[derive(Debug)]
pub enum ClipboardError {
	Timeout,
	/// The clipboard, with which this program is trying to communicate, fails to uphold the agreed behaviour.
	ForeignClipboardError,
	ClipboardDataUnavailable,
	AllocationFailed,
	SetFailed,
	OpenFailed,
	FormatNotAvailable,
	LockFailed,
	ReadFailed,
	Utf16ConversionFailed,
	Empty,
	InUse,
	WriteFailed,
	UnsupportedMimeType,
	#[cfg(not(target_arch = "wasm32"))]
	Unknown(String),
}

#[cfg(target_arch = "wasm32")]
impl ClipboardError {
	pub fn try_from(code: u32) -> Option<Self> {
		match code {
			0 => Some(Self::Timeout),
			1 => Some(Self::ForeignClipboardError),
			2 => Some(Self::ClipboardDataUnavailable),
			3 => Some(Self::AllocationFailed),
			4 => Some(Self::SetFailed),
			5 => Some(Self::OpenFailed),
			6 => Some(Self::FormatNotAvailable),
			7 => Some(Self::LockFailed),
			8 => Some(Self::ReadFailed),
			9 => Some(Self::Utf16ConversionFailed),
			10 => Some(Self::Empty),
			11 => Some(Self::InUse),
			12 => Some(Self::WriteFailed),
			13 => Some(Self::UnsupportedMimeType),
			_ => None,
		}
	}
}

#[cfg(target_arch = "wasm32")]
impl From<ClipboardError> for u32 {
	fn from(value: ClipboardError) -> Self {
		match value {
			ClipboardError::Timeout => 0,
			ClipboardError::ForeignClipboardError => 1,
			ClipboardError::ClipboardDataUnavailable => 2,
			ClipboardError::AllocationFailed => 3,
			ClipboardError::SetFailed => 4,
			ClipboardError::OpenFailed => 5,
			ClipboardError::FormatNotAvailable => 6,
			ClipboardError::LockFailed => 7,
			ClipboardError::ReadFailed => 8,
			ClipboardError::Utf16ConversionFailed => 9,
			ClipboardError::Empty => 10,
			ClipboardError::InUse => 11,
			ClipboardError::WriteFailed => 12,
			ClipboardError::UnsupportedMimeType => 13,
		}
	}
}
