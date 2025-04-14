//! Basic framebuffer driver for printing stuff to screen

use limine::framebuffer::Framebuffer;

/// Errors the framebuffer might encounter
#[derive(Debug, Clone, Copy)]
pub enum FramebufferError {
    /// Encountered an invalid character
    InvalidCharacter,
}

/// A bitmap font for the framebuffer
#[rustfmt::skip]
static BITMAP_FONT_8X16: [[u8; 16]; 256] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x6c, 0x6c, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x36, 0x36, 0x7f, 0x36, 0x36, 0x7f, 0x36, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x08, 0x3e, 0x6b, 0x0b, 0x0b, 0x3e, 0x68, 0x68, 0x6b, 0x3e, 0x08, 0x08, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x33, 0x13, 0x18, 0x08, 0x0c, 0x04, 0x06, 0x32, 0x33, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x1c, 0x36, 0x36, 0x1c, 0x6c, 0x3e, 0x33, 0x33, 0x7b, 0xce, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x18, 0x18, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x30, 0x18, 0x18, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x0c, 0x18, 0x18, 0x30, 0x30, 0x30, 0x30, 0x30, 0x18, 0x18, 0x0c, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x1c, 0x7f, 0x1c, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x0c, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x60, 0x20, 0x30, 0x10, 0x18, 0x08, 0x0c, 0x04, 0x06, 0x02, 0x03, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x6b, 0x6b, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x18, 0x1e, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x60, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x60, 0x60, 0x3c, 0x60, 0x60, 0x60, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x30, 0x38, 0x3c, 0x36, 0x33, 0x7f, 0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7f, 0x03, 0x03, 0x3f, 0x60, 0x60, 0x60, 0x60, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x06, 0x03, 0x03, 0x3f, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7f, 0x60, 0x30, 0x30, 0x18, 0x18, 0x18, 0x0c, 0x0c, 0x0c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x7e, 0x60, 0x60, 0x60, 0x30, 0x1e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x0c, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x60, 0x30, 0x30, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x66, 0x73, 0x7b, 0x6b, 0x6b, 0x7b, 0x33, 0x06, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3f, 0x63, 0x63, 0x63, 0x3f, 0x63, 0x63, 0x63, 0x63, 0x3f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x66, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x66, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x1f, 0x33, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x33, 0x1f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x66, 0x03, 0x03, 0x03, 0x73, 0x63, 0x63, 0x66, 0x7c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x33, 0x1e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x33, 0x1b, 0x0f, 0x07, 0x07, 0x0f, 0x1b, 0x33, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x77, 0x7f, 0x7f, 0x6b, 0x6b, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x67, 0x6f, 0x6f, 0x7b, 0x7b, 0x73, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3f, 0x63, 0x63, 0x63, 0x63, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x6f, 0x7b, 0x3e, 0x30, 0x60, 0x00, 0x00,],
    [0x00, 0x00, 0x3f, 0x63, 0x63, 0x63, 0x63, 0x3f, 0x1b, 0x33, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3e, 0x63, 0x03, 0x03, 0x0e, 0x38, 0x60, 0x60, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7e, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x08, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x6b, 0x6b, 0x6b, 0x6b, 0x7f, 0x36, 0x36, 0x36, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x36, 0x36, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0xc3, 0xc3, 0x66, 0x66, 0x3c, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x7f, 0x30, 0x30, 0x18, 0x18, 0x0c, 0x0c, 0x06, 0x06, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x03, 0x02, 0x06, 0x04, 0x0c, 0x08, 0x18, 0x10, 0x30, 0x20, 0x60, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00,],
    [0x00, 0x00, 0x0c, 0x0c, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x03, 0x03, 0x03, 0x3b, 0x67, 0x63, 0x63, 0x63, 0x67, 0x3b, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x63, 0x03, 0x03, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x60, 0x60, 0x60, 0x6e, 0x73, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x63, 0x63, 0x7f, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x66, 0x06, 0x1f, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x73, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x60, 0x63, 0x3e, 0x00,],
    [0x00, 0x00, 0x03, 0x03, 0x03, 0x3b, 0x67, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x30, 0x30, 0x00, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x33, 0x1e, 0x00,],
    [0x00, 0x00, 0x03, 0x03, 0x03, 0x63, 0x33, 0x1b, 0x0f, 0x1f, 0x33, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x35, 0x6b, 0x6b, 0x6b, 0x6b, 0x6b, 0x6b, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3b, 0x67, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3b, 0x67, 0x63, 0x63, 0x63, 0x67, 0x3b, 0x03, 0x03, 0x03, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x73, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x60, 0xe0, 0x60, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3b, 0x67, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x63, 0x0e, 0x38, 0x60, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x0c, 0x0c, 0x3e, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x08, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x63, 0x6b, 0x6b, 0x6b, 0x3e, 0x36, 0x36, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x63, 0x36, 0x1c, 0x1c, 0x1c, 0x36, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x0c, 0x0c, 0x06, 0x03, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x70, 0x18, 0x18, 0x18, 0x18, 0x0e, 0x18, 0x18, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00,],
    [0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x0e, 0x18, 0x18, 0x18, 0x18, 0x70, 0x18, 0x18, 0x18, 0x18, 0x0e, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x3b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x08, 0x08, 0x3e, 0x6b, 0x0b, 0x0b, 0x0b, 0x6b, 0x3e, 0x08, 0x08, 0x00, 0x00,],
    [0x00, 0x00, 0x1c, 0x36, 0x06, 0x06, 0x1f, 0x06, 0x06, 0x07, 0x6f, 0x3b, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x66, 0x3c, 0x66, 0x66, 0x66, 0x3c, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0xc3, 0xc3, 0x66, 0x66, 0x3c, 0x7e, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x3c, 0x66, 0x0c, 0x1e, 0x33, 0x63, 0x66, 0x3c, 0x18, 0x33, 0x1e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x42, 0x99, 0xa5, 0x85, 0xa5, 0x99, 0x42, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x1e, 0x30, 0x3e, 0x33, 0x3b, 0x36, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x6c, 0x36, 0x1b, 0x1b, 0x36, 0x6c, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x42, 0x9d, 0xa5, 0x9d, 0xa5, 0xa5, 0x42, 0x3c, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x1c, 0x36, 0x36, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x1e, 0x33, 0x18, 0x0c, 0x06, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x1e, 0x33, 0x18, 0x30, 0x33, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x76, 0x6e, 0x06, 0x06, 0x03, 0x00,],
    [0x00, 0x00, 0x7e, 0x2f, 0x2f, 0x2f, 0x2e, 0x28, 0x28, 0x28, 0x28, 0x28, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x30, 0x1e, 0x00,],
    [0x00, 0x0c, 0x0e, 0x0c, 0x0c, 0x0c, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x1e, 0x33, 0x33, 0x33, 0x33, 0x1e, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0x36, 0x6c, 0x6c, 0x36, 0x1b, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x10, 0x1c, 0x18, 0x18, 0x18, 0x00, 0x7f, 0x00, 0x18, 0x1c, 0x1a, 0x3e, 0x18, 0x00, 0x00,],
    [0x00, 0x10, 0x1c, 0x18, 0x18, 0x18, 0x00, 0x7f, 0x00, 0x1c, 0x36, 0x18, 0x0c, 0x3e, 0x00, 0x00,],
    [0x00, 0x1c, 0x36, 0x18, 0x36, 0x1c, 0x00, 0x7f, 0x00, 0x18, 0x1c, 0x1a, 0x3e, 0x18, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x06, 0x06, 0x03, 0x63, 0x3e, 0x00, 0x00,],
    [0x0c, 0x18, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x18, 0x0c, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x08, 0x14, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x6e, 0x3b, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x36, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x1c, 0x36, 0x3e, 0x63, 0x63, 0x63, 0x7f, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0xfe, 0x33, 0x33, 0x33, 0xff, 0x33, 0x33, 0x33, 0x33, 0xf3, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x3c, 0x66, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x66, 0x3c, 0x18, 0x30, 0x1e, 0x00,],
    [0x0c, 0x18, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x18, 0x0c, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x08, 0x14, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x36, 0x00, 0x7f, 0x03, 0x03, 0x03, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x7f, 0x00, 0x00, 0x00, 0x00,],
    [0x0c, 0x18, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x30, 0x18, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x18, 0x24, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x66, 0x00, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x1e, 0x36, 0x66, 0x66, 0x6f, 0x66, 0x66, 0x66, 0x36, 0x1e, 0x00, 0x00, 0x00, 0x00,],
    [0x6e, 0x3b, 0x63, 0x63, 0x67, 0x6f, 0x6f, 0x7b, 0x7b, 0x73, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x06, 0x0c, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x30, 0x18, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x08, 0x14, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x6e, 0x3b, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x36, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x3c, 0x18, 0x3c, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x20, 0x3e, 0x73, 0x73, 0x6b, 0x6b, 0x6b, 0x6b, 0x67, 0x67, 0x3e, 0x02, 0x00, 0x00, 0x00,],
    [0x0c, 0x18, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x18, 0x0c, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x08, 0x14, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x36, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x30, 0x18, 0xc3, 0xc3, 0x66, 0x66, 0x3c, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x0f, 0x06, 0x3e, 0x66, 0x66, 0x66, 0x66, 0x3e, 0x06, 0x0f, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x1e, 0x33, 0x33, 0x1b, 0x33, 0x63, 0x63, 0x63, 0x63, 0x3b, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x0c, 0x18, 0x30, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x6e, 0x3b, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x1c, 0x36, 0x1c, 0x00, 0x3e, 0x60, 0x7e, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0xdb, 0xd8, 0xfe, 0x1b, 0xdb, 0x76, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x63, 0x03, 0x03, 0x03, 0x63, 0x3e, 0x18, 0x30, 0x1e, 0x00,],
    [0x00, 0x0c, 0x18, 0x30, 0x00, 0x3e, 0x63, 0x63, 0x7f, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x3e, 0x63, 0x63, 0x7f, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x00, 0x3e, 0x63, 0x63, 0x7f, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x3e, 0x63, 0x63, 0x7f, 0x03, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x06, 0x0c, 0x18, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x18, 0x0c, 0x06, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x38, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x2c, 0x18, 0x34, 0x60, 0x7c, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x6e, 0x3b, 0x00, 0x3b, 0x67, 0x63, 0x63, 0x63, 0x63, 0x63, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x06, 0x0c, 0x18, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x6e, 0x3b, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x3e, 0x63, 0x63, 0x63, 0x63, 0x63, 0x3e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x7e, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x00, 0x00, 0x20, 0x3e, 0x73, 0x6b, 0x6b, 0x6b, 0x67, 0x3e, 0x02, 0x00, 0x00, 0x00,],
    [0x00, 0x06, 0x0c, 0x18, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x08, 0x1c, 0x36, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x63, 0x63, 0x63, 0x63, 0x63, 0x73, 0x6e, 0x00, 0x00, 0x00, 0x00,],
    [0x00, 0x30, 0x18, 0x0c, 0x00, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x0c, 0x0c, 0x06, 0x03, 0x00,],
    [0x00, 0x00, 0x0f, 0x06, 0x06, 0x3e, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3e, 0x06, 0x06, 0x0f, 0x00,],
    [0x00, 0x00, 0x36, 0x36, 0x00, 0x63, 0x63, 0x36, 0x36, 0x1c, 0x1c, 0x0c, 0x0c, 0x06, 0x03, 0x00,], 
];

/// The raw framebuffer. Used only by FramebufferWriter.
struct RawFramebuffer {
    ptr: *mut u32,
    width: u64,
    height: u64,
    pitch: u64,
    bpp: u16,
}

/// The FramebufferWriter struct is used to write to the framebuffer.
pub struct FramebufferWriter {
    framebuffer: RawFramebuffer,
    curr_y: u64,
    curr_x: u64,
    disabled: bool,
}

pub static mut FRAMEBUFFER_WRITER: FramebufferWriter = FramebufferWriter {
    framebuffer: RawFramebuffer {
        ptr: core::ptr::null_mut(),
        width: 0,
        height: 0,
        pitch: 0,
        bpp: 0,
    },
    curr_y: 0,
    curr_x: 0,
    disabled: false,
};

impl FramebufferWriter {
    /// Initializes the framebuffer writer with the given framebuffer using Limine's `Framebuffer`
    /// structure.
    #[cfg(feature = "limine")]
    #[inline]
    pub fn init_from_limine(&mut self, fb: Framebuffer<'static>) {
        self.framebuffer.ptr = fb.addr() as *mut u32;
        self.framebuffer.width = fb.width();
        self.framebuffer.height = fb.height();
        self.framebuffer.pitch = fb.pitch();
        self.framebuffer.bpp = fb.bpp();
    }

    /// Draws a pixel at the given coordinates with the given color.
    pub(super) fn draw_pixel(&self, mut x: u64, mut y: u64, color: u32) {
        y *= self.framebuffer.pitch;
        x *= (self.framebuffer.bpp / 8) as u64;

        unsafe { *(self.framebuffer.ptr.byte_add((x + y) as usize)) = color };
    }

    /// Increments the current y cursor one character line down (16 pixels).
    fn scroll_y(&mut self) {
        self.curr_y = self.curr_y + 16;

        if self.curr_y >= self.framebuffer.height {
            self.disabled = true;
        }
    }

    /// Increments the current x cursor one character line right (8 pixels). Increments the y
    /// cursor if the x cursor is at the end of the line, or if the newly written character is a
    /// newline.
    fn scroll_x(&mut self, character: u8) {
        self.curr_x += 8;
        if self.curr_x >= self.framebuffer.width || character == b'\n' {
            self.curr_x = 0;
            self.scroll_y();
        }
    }

    /// Draws a character at the current cursor position. 
    /// The character is drawn using the 8x16 bitmap font.
    pub(super) fn draw_char(&mut self, character: u8) -> Result<(), FramebufferError> {
        if self.disabled {
            return Ok(());
        }

        let index = character as usize;
        if index >= BITMAP_FONT_8X16.len() {
            return Err(FramebufferError::InvalidCharacter);
        }

        for y in 0..16 {
            let char_bit = BITMAP_FONT_8X16[index][y];
            for x in 0..8 {
                if (char_bit >> x) & 0x1 != 0 {
                    self.draw_pixel(x + self.curr_x, y as u64 + self.curr_y, 0xffffff);
                }
            }
        }

        self.scroll_x(character);

        Ok(())
    }
}
