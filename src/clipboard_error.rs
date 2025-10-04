#[derive(Debug)]
pub enum ClipboardError {
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
	Unknown(String),
}
