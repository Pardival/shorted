//! Composes frames from `core::evaluate` (ADR-0003): the one renderer for preview and export.
//! Clips, crop, rotation and crossfade for now; overlays and text come with T-0402.

use bytemuck::{Pod, Zeroable};

use crate::spike::{RenderError, TARGET_FORMAT, target_descriptor};

const MAX_LAYERS: u64 = 2;
const LAYER_STRIDE: u64 = 256;
const BYTES_PER_PIXEL: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerUniform {
    scale: [f32; 2],
    offset: [f32; 2],
    quarter_turns: u32,
    opacity: f32,
    shift: [f32; 2],
    tint: [f32; 4],
    brightness: f32,
    contrast: f32,
    saturation: f32,
    frame_zoom: f32,
}

/// What a clip's colour and a transition do to a layer. The default changes nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerEffect {
    pub opacity: f32,
    pub shift: [f32; 2],
    pub tint: [f32; 4],
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub frame_zoom: f32,
}

impl Default for LayerEffect {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            shift: [0.0; 2],
            tint: [0.0; 4],
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            frame_zoom: 1.0,
        }
    }
}

/// How to place one source in the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPlacement {
    /// Size of the source as stored.
    pub width: u32,
    pub height: u32,
    /// Clockwise turns to display it, from the file's rotation metadata.
    pub quarter_turns: u32,
    /// Crop: centre in 0..1 of the displayed picture, and zoom.
    pub center: [f32; 2],
    pub zoom: f32,
    pub effect: LayerEffect,
}

impl LayerPlacement {
    /// Centre-crops to fill the frame, then applies crop zoom and position.
    fn uniform(&self, frame_width: u32, frame_height: u32) -> LayerUniform {
        let turned = self.quarter_turns % 2 == 1;
        let (shown_width, shown_height) = if turned {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let source = shown_width as f32 / shown_height.max(1) as f32;
        let frame = frame_width as f32 / frame_height.max(1) as f32;
        let (x, y) = if source > frame {
            (frame / source, 1.0)
        } else {
            (1.0, source / frame)
        };
        let zoom = self.zoom.max(0.01);
        let scale = [x / zoom, y / zoom];
        let offset = [
            self.center[0].clamp(scale[0] / 2.0, 1.0 - scale[0] / 2.0),
            self.center[1].clamp(scale[1] / 2.0, 1.0 - scale[1] / 2.0),
        ];
        LayerUniform {
            scale,
            offset,
            quarter_turns: self.quarter_turns % 4,
            opacity: self.effect.opacity,
            shift: self.effect.shift,
            tint: self.effect.tint,
            brightness: self.effect.brightness,
            contrast: self.effect.contrast,
            saturation: self.effect.saturation,
            frame_zoom: self.effect.frame_zoom.max(0.01),
        }
    }
}

pub struct Compositor {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub width: u32,
    pub height: u32,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
}

impl Compositor {
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        pollster::block_on(Self::create(width, height))
    }

    async fn create(width: u32, height: u32) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;
        let shader = device.create_shader_module(wgpu::include_wgsl!("compositor.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size_of::<LayerUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compositor layer"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor layers"),
            size: LAYER_STRIDE * MAX_LAYERS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            width,
            height,
            pipeline,
            layout,
            sampler,
            uniforms,
        })
    }

    /// A texture holding RGBA pixels, for images and placeholders.
    pub fn texture_from_rgba(&self, width: u32, height: u32, rgba: &[u8]) -> wgpu::Texture {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compositor image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * BYTES_PER_PIXEL),
                rows_per_image: None,
            },
            size,
        );
        texture
    }

    fn encode(
        &self,
        layers: &[(&wgpu::Texture, LayerPlacement)],
        target: &wgpu::Texture,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let groups: Vec<wgpu::BindGroup> = layers
            .iter()
            .take(MAX_LAYERS as usize)
            .enumerate()
            .map(|(index, (texture, placement))| {
                let uniform = placement.uniform(self.width, self.height);
                self.queue.write_buffer(
                    &self.uniforms,
                    index as u64 * LAYER_STRIDE,
                    bytemuck::bytes_of(&uniform),
                );
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.uniforms,
                                offset: 0,
                                size: wgpu::BufferSize::new(size_of::<LayerUniform>() as u64),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        for (index, group) in groups.iter().enumerate() {
            pass.set_bind_group(0, group, &[index as u32 * LAYER_STRIDE as u32]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Composes `layers`, bottom first, into `target` and waits for the GPU.
    pub fn render(
        &self,
        layers: &[(&wgpu::Texture, LayerPlacement)],
        target: &wgpu::Texture,
    ) -> Result<(), RenderError> {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(layers, target, &mut encoder);
        self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    /// Composes offscreen and returns BGRA pixels: for tests, and for export later.
    pub fn render_to_pixels(
        &self,
        layers: &[(&wgpu::Texture, LayerPlacement)],
    ) -> Result<Vec<u8>, RenderError> {
        let target = self
            .device
            .create_texture(&target_descriptor(self.width, self.height));
        let row =
            (self.width * BYTES_PER_PIXEL).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compositor readback"),
            size: u64::from(row * self.height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(layers, &target, &mut encoder);
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        if let Ok(result) = receiver.recv() {
            result?;
        }
        let mapped = slice.get_mapped_range()?;
        let tight = (self.width * BYTES_PER_PIXEL) as usize;
        let mut pixels = Vec::with_capacity(tight * self.height as usize);
        for line in mapped.chunks_exact(row as usize) {
            pixels.extend_from_slice(&line[..tight]);
        }
        Ok(pixels)
    }
}

/// Decodes an image file to RGBA pixels.
pub fn load_image(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let picture = image::open(path).ok()?.into_rgba8();
    let (width, height) = picture.dimensions();
    Some((width, height, picture.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: u32 = 90;
    const HEIGHT: u32 = 160;

    fn compositor() -> Option<Compositor> {
        match Compositor::new(WIDTH, HEIGHT) {
            Ok(compositor) => Some(compositor),
            // CI machines may have no GPU.
            Err(RenderError::NoAdapter(_)) => None,
            Err(error) => panic!("{error}"),
        }
    }

    fn solid(compositor: &Compositor, rgb: [u8; 3]) -> wgpu::Texture {
        let pixels: Vec<u8> = [rgb[0], rgb[1], rgb[2], 255].repeat(4);
        compositor.texture_from_rgba(2, 2, &pixels)
    }

    /// 2×2: red, green on top; blue, white below.
    fn quadrants(compositor: &Compositor) -> wgpu::Texture {
        let pixels = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        compositor.texture_from_rgba(2, 2, &pixels)
    }

    fn placement() -> LayerPlacement {
        LayerPlacement {
            width: 90,
            height: 160,
            quarter_turns: 0,
            center: [0.5, 0.5],
            zoom: 1.0,
            effect: LayerEffect::default(),
        }
    }

    /// (red, green, blue) at a point given in 0..1 of the frame.
    fn rgb_at(pixels: &[u8], x: f32, y: f32) -> [u8; 3] {
        let column = (x * WIDTH as f32) as usize;
        let row = (y * HEIGHT as f32) as usize;
        let at = (row * WIDTH as usize + column) * 4;
        [pixels[at + 2], pixels[at + 1], pixels[at]]
    }

    fn close(actual: [u8; 3], expected: [u8; 3]) -> bool {
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2)
    }

    #[test]
    fn a_picture_keeps_its_orientation() {
        let Some(compositor) = compositor() else {
            return;
        };
        let texture = quadrants(&compositor);
        let pixels = compositor
            .render_to_pixels(&[(&texture, placement())])
            .unwrap();
        assert!(close(rgb_at(&pixels, 0.1, 0.1), [255, 0, 0]));
        assert!(close(rgb_at(&pixels, 0.9, 0.1), [0, 255, 0]));
        assert!(close(rgb_at(&pixels, 0.1, 0.9), [0, 0, 255]));
        assert!(close(rgb_at(&pixels, 0.9, 0.9), [255, 255, 255]));
    }

    #[test]
    fn a_quarter_turn_rotates_clockwise() {
        let Some(compositor) = compositor() else {
            return;
        };
        let texture = quadrants(&compositor);
        let turned = LayerPlacement {
            width: 160,
            height: 90,
            quarter_turns: 1,
            ..placement()
        };
        let pixels = compositor.render_to_pixels(&[(&texture, turned)]).unwrap();
        // Top-left of the source lands top-right.
        assert!(close(rgb_at(&pixels, 0.9, 0.1), [255, 0, 0]));
        assert!(close(rgb_at(&pixels, 0.9, 0.9), [0, 255, 0]));
        assert!(close(rgb_at(&pixels, 0.1, 0.1), [0, 0, 255]));
    }

    #[test]
    fn half_turn_turns_the_picture_over() {
        let Some(compositor) = compositor() else {
            return;
        };
        let texture = quadrants(&compositor);
        let turned = LayerPlacement {
            quarter_turns: 2,
            ..placement()
        };
        let pixels = compositor.render_to_pixels(&[(&texture, turned)]).unwrap();
        assert!(close(rgb_at(&pixels, 0.9, 0.9), [255, 0, 0]));
    }

    #[test]
    fn a_crossfade_mixes_two_layers() {
        let Some(compositor) = compositor() else {
            return;
        };
        let black = solid(&compositor, [0, 0, 0]);
        let white = solid(&compositor, [255, 255, 255]);
        let top = LayerPlacement {
            effect: LayerEffect {
                opacity: 0.5,
                ..LayerEffect::default()
            },
            ..placement()
        };
        let pixels = compositor
            .render_to_pixels(&[(&black, placement()), (&white, top)])
            .unwrap();
        assert!(close(rgb_at(&pixels, 0.5, 0.5), [128, 128, 128]));
    }

    #[test]
    fn landscape_is_cropped_to_fill_portrait() {
        let wide = LayerPlacement {
            width: 1920,
            height: 1080,
            ..placement()
        };
        let uniform = wide.uniform(1080, 1920);
        assert!((uniform.scale[0] - 0.316).abs() < 0.001);
        assert_eq!(uniform.scale[1], 1.0);
    }

    #[test]
    fn crop_stays_inside_the_source() {
        let moved = LayerPlacement {
            zoom: 2.0,
            center: [0.0, 1.0],
            ..placement()
        };
        let uniform = moved.uniform(90, 160);
        assert_eq!(uniform.offset, [0.25, 0.75]);
    }

    /// The middle pixel of a solid colour drawn with `effect`.
    fn drawn(rgb: [u8; 3], effect: LayerEffect) -> Option<[u8; 3]> {
        let compositor = compositor()?;
        let texture = solid(&compositor, rgb);
        let layer = LayerPlacement {
            effect,
            ..placement()
        };
        let pixels = compositor.render_to_pixels(&[(&texture, layer)]).ok()?;
        Some(rgb_at(&pixels, 0.5, 0.5))
    }

    #[test]
    fn colour_adjustments_change_the_picture() {
        let grey = [128, 128, 128];
        let plain = LayerEffect::default();
        let Some(same) = drawn(grey, plain) else {
            return;
        };
        assert!(close(same, grey), "{same:?}");
        let bright = LayerEffect {
            brightness: 0.25,
            ..plain
        };
        assert!(close(drawn(grey, bright).unwrap(), [192, 192, 192]));
        let flat = LayerEffect {
            contrast: 0.0,
            ..plain
        };
        assert!(close(drawn([255, 0, 0], flat).unwrap(), [128, 128, 128]));
        let pale = LayerEffect {
            saturation: 0.0,
            ..plain
        };
        let luma = drawn([255, 0, 0], pale).unwrap();
        assert!(close(luma, [54, 54, 54]), "{luma:?}");
    }

    #[test]
    fn a_tint_covers_the_picture() {
        let white = LayerEffect {
            tint: [1.0, 1.0, 1.0, 1.0],
            ..LayerEffect::default()
        };
        if let Some(found) = drawn([10, 20, 30], white) {
            assert!(close(found, [255, 255, 255]), "{found:?}");
        }
    }

    #[test]
    fn a_shifted_picture_leaves_the_frame() {
        let gone = LayerEffect {
            shift: [1.0, 0.0],
            ..LayerEffect::default()
        };
        if let Some(found) = drawn([200, 200, 200], gone) {
            assert!(close(found, [0, 0, 0]), "{found:?}");
        }
    }

    #[test]
    fn extra_layers_are_ignored_without_crashing() {
        let Some(compositor) = compositor() else {
            return;
        };
        let texture = solid(&compositor, [9, 9, 9]);
        let layers = [
            (&texture, placement()),
            (&texture, placement()),
            (&texture, placement()),
        ];
        assert!(compositor.render_to_pixels(&layers).is_ok());
    }
}
