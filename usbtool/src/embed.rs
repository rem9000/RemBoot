//! Optionally embed BOOTX64.EFI into the binary so the tool is self-contained
//! (one download, no separate app file). The release builds with
//! `--features embed-efi` after copying the built app to
//! `usbtool/embedded-efi.bin`.

#[cfg(feature = "embed-efi")]
pub const EFI: Option<&[u8]> =
    Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/embedded-efi.bin")));

#[cfg(not(feature = "embed-efi"))]
pub const EFI: Option<&[u8]> = None;
