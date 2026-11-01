//! Zero-copy target on Apple platforms: a wgpu texture over an IOSurface also read by Flutter.

use objc2_io_surface::IOSurfaceRef;
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use wgpu::hal::api::Metal;

use crate::spike::{RenderError, target_descriptor};

/// Wraps `surface` in a wgpu texture with the given usage.
///
/// # Safety
///
/// `surface` must be a valid BGRA8 IOSurface of `width` × `height` pixels, and must stay alive
/// as long as the returned texture.
pub unsafe fn texture_from_iosurface(
    device: &wgpu::Device,
    surface: &IOSurfaceRef,
    width: u32,
    height: u32,
    usage: wgpu::TextureUsages,
) -> Result<wgpu::Texture, RenderError> {
    // SAFETY: this device was created by wgpu on the Metal backend, the only one on Apple here.
    let hal_device = unsafe { device.as_hal::<Metal>() }.ok_or(RenderError::NotMetal)?;
    let raw_device = hal_device.raw_device();
    // SAFETY: plain Objective-C object creation with valid dimensions.
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    descriptor.setStorageMode(MTLStorageMode::Shared);
    let raw = raw_device
        .newTextureWithDescriptor_iosurface_plane(&descriptor, surface, 0)
        .ok_or(RenderError::SharedTexture)?;
    // SAFETY: `raw` matches the format, type and size given here and `descriptor` below.
    let hal_texture = unsafe {
        wgpu::hal::metal::Device::texture_from_raw(
            raw,
            crate::spike::TARGET_FORMAT,
            MTLTextureType::Type2D,
            1,
            1,
            wgpu::hal::CopyExtent {
                width,
                height,
                depth: 1,
            },
            None,
        )
    };
    drop(hal_device);
    // SAFETY: the hal texture was created from this device and is not used elsewhere by wgpu.
    Ok(unsafe {
        device.create_texture_from_hal::<Metal>(
            hal_texture,
            &wgpu::TextureDescriptor {
                usage,
                ..target_descriptor(width, height)
            },
            wgpu::TextureUses::UNINITIALIZED,
        )
    })
}
