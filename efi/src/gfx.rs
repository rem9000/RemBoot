//! Software backbuffer + GOP presentation.
//!
//! All composition happens in RAM (see `remboot_core::ui::compose`) on a
//! `Vec<u32>` of `0x00RRGGBB` pixels; a single full-frame `Blt` presents it.
//! `BltPixel` is `{blue, green, red, reserved}` in memory, which is exactly
//! the little-endian byte layout of a `0x00RRGGBB` u32, so the buffer can be
//! reinterpreted without conversion.

use alloc::vec;
use alloc::vec::Vec;
use uefi::Result;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};

const _: () = assert!(core::mem::size_of::<BltPixel>() == 4);

pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![0u32; w * h] }
    }

    pub fn present(&self, gop: &mut GraphicsOutput) -> Result {
        let buf: &[BltPixel] =
            unsafe { core::slice::from_raw_parts(self.px.as_ptr().cast(), self.px.len()) };
        gop.blt(BltOp::BufferToVideo {
            buffer: buf,
            src: BltRegion::Full,
            dest: (0, 0),
            dims: (self.w, self.h),
        })
    }
}
