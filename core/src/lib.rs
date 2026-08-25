//! remboot-core: pure logic shared by the UEFI app and host-side tests.
//!
//! Everything in here is `no_std` + `alloc` and free of UEFI dependencies,
//! so it can be unit-tested with a plain `cargo test` on the host.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod catalog;
pub mod exfat;
pub mod fx;
pub mod menu;
pub mod pix;
pub mod scene;
pub mod text;
pub mod theme;
pub mod ui;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
