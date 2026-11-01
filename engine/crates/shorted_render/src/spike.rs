//! T-0005 spike: an animated test scene (background, 3 moving overlays, captions text) rendered
//! with wgpu. It measures the path to Flutter; it is not the production compositor (T-0402).

use bytemuck::{Pod, Zeroable};
use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache};
use thiserror::Error;

pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const BYTES_PER_PIXEL: u32 = 4;
const TEXT_WIDTH: u32 = 900;
const TEXT_HEIGHT: u32 = 220;
const CAPTION_SECONDS: f32 = 0.5;
const CAPTION_WORDS: [&str; 4] = ["FLUID", "OR IT", "DOESN'T", "SHIP"];

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("no GPU adapter is available")]
    NoAdapter(#[from] wgpu::RequestAdapterError),
    #[error("the GPU device could not be created")]
    NoDevice(#[from] wgpu::RequestDeviceError),
    #[error("the GPU did not finish in time")]
    Timeout(#[from] wgpu::PollError),
    #[error("pixels could not be read back")]
    Readback(#[from] wgpu::BufferAsyncError),
    #[error("pixels could not be mapped")]
    Mapping(#[from] wgpu::MapRangeError),
    #[error("the device does not use Metal")]
    NotMetal,
    #[error("the shared texture could not be created")]
    SharedTexture,
    #[error("the destination buffer is too small")]
    DestinationTooSmall,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    time: f32,
    aspect: f32,
    uv_scale: [f32; 2],
    rotated: f32,
    padding: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadInstance {
    center: [f32; 2],
    half_size: [f32; 2],
    rotation: f32,
    textured: f32,
    color: [f32; 4],
}

/// CPU-side rasteriser for captions. Text is shaped in Rust, as ADR-0003 requires.
struct CaptionRaster {
    fonts: FontSystem,
    cache: SwashCache,
    buffer: Buffer,
    pixels: Vec<u8>,
}

impl CaptionRaster {
    fn new() -> Self {
        let mut fonts = FontSystem::new();
        let buffer = Buffer::new(&mut fonts, Metrics::new(150.0, 170.0));
        Self {
            fonts,
            cache: SwashCache::new(),
            buffer,
            pixels: vec![0; (TEXT_WIDTH * TEXT_HEIGHT) as usize],
        }
    }

    fn rasterise(&mut self, text: &str) -> &[u8] {
        self.pixels.fill(0);
        self.buffer
            .set_size(Some(TEXT_WIDTH as f32), Some(TEXT_HEIGHT as f32));
        self.buffer
            .set_text(text, &Attrs::new(), Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.fonts, false);
        let pixels = &mut self.pixels;
        self.buffer.draw(
            &mut self.fonts,
            &mut self.cache,
            Color::rgb(255, 255, 255),
            |x, y, width, height, color| {
                for row in y.max(0)..(y + height as i32).min(TEXT_HEIGHT as i32) {
                    for column in x.max(0)..(x + width as i32).min(TEXT_WIDTH as i32) {
                        let index = row as usize * TEXT_WIDTH as usize + column as usize;
                        pixels[index] = pixels[index].max(color.a());
                    }
                }
            },
        );
        &self.pixels
    }
}

pub struct SpikeRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub width: u32,
    pub height: u32,
    background: wgpu::RenderPipeline,
    video_background: wgpu::RenderPipeline,
    video_layout: wgpu::BindGroupLayout,
    video_sampler: wgpu::Sampler,
    video: Option<wgpu::BindGroup>,
    uv_scale: [f32; 2],
    rotated: bool,
    quads: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    text_texture: wgpu::Texture,
    caption: CaptionRaster,
    caption_index: Option<usize>,
    offscreen: wgpu::Texture,
    readback: wgpu::Buffer,
}

impl SpikeRenderer {
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("spike.shader.wgsl"));

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike quads"),
            size: (size_of::<QuadInstance>() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let text_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("spike captions"),
            size: wgpu::Extent3d {
                width: TEXT_WIDTH,
                height: TEXT_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
        let text_view = text_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&text_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let video_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let video_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout), Some(&video_layout)],
                immediate_size: 0,
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32, 4 => Float32x4
        ];
        let quad_layouts = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<QuadInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attributes,
        })];
        let pipeline =
            |layout: &wgpu::PipelineLayout, vs: &str, fs: &str, buffers, blend, topology| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: TARGET_FORMAT,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let video_background = pipeline(
            &video_pipeline_layout,
            "background_vs",
            "video_fs",
            &[],
            None,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let background = pipeline(
            &pipeline_layout,
            "background_vs",
            "background_fs",
            &[],
            None,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let quads = pipeline(
            &pipeline_layout,
            "quad_vs",
            "quad_fs",
            &quad_layouts,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            wgpu::PrimitiveTopology::TriangleStrip,
        );
        let offscreen = device.create_texture(&target_descriptor(width, height));
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike readback"),
            size: u64::from(padded_row(width) * height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            width,
            height,
            background,
            video_background,
            video_layout,
            video_sampler: sampler,
            video: None,
            uv_scale: [1.0, 1.0],
            rotated: false,
            quads,
            bind_group,
            globals,
            instances,
            text_texture,
            caption: CaptionRaster::new(),
            caption_index: None,
            offscreen,
            readback,
        })
    }

    /// Uses a decoded frame as the background of the next renders. `display_width` and
    /// `display_height` include rotation.
    pub fn set_video_frame(
        &mut self,
        texture: &wgpu::Texture,
        display_width: u32,
        display_height: u32,
        rotated: bool,
    ) {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.video = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.video_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.video_sampler),
                },
            ],
        }));
        self.rotated = rotated;
        let clip = display_width as f32 / display_height as f32;
        let frame = self.width as f32 / self.height as f32;
        // In the sample's own axes: a wide clip is cropped along its width.
        let (cross, along) = if clip > frame {
            (frame / clip, 1.0)
        } else {
            (1.0, clip / frame)
        };
        self.uv_scale = if rotated {
            [along, cross]
        } else {
            [cross, along]
        };
    }

    fn update_scene(&mut self, time: f32) {
        let caption_index = (time / CAPTION_SECONDS) as usize % CAPTION_WORDS.len();
        if self.caption_index != Some(caption_index) {
            self.caption_index = Some(caption_index);
            let pixels = self.caption.rasterise(CAPTION_WORDS[caption_index]);
            self.queue.write_texture(
                self.text_texture.as_image_copy(),
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TEXT_WIDTH),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: TEXT_WIDTH,
                    height: TEXT_HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
        }
        let aspect = self.width as f32 / self.height as f32;
        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                time,
                aspect,
                uv_scale: self.uv_scale,
                rotated: if self.rotated { 1.0 } else { 0.0 },
                padding: [0.0; 3],
            }),
        );
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&scene_quads(time)));
    }

    fn encode(&self, target: &wgpu::Texture, encoder: &mut wgpu::CommandEncoder) {
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("spike scene"),
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
        pass.set_bind_group(0, &self.bind_group, &[]);
        match &self.video {
            Some(video) => {
                pass.set_bind_group(1, video, &[]);
                pass.set_pipeline(&self.video_background);
            }
            None => pass.set_pipeline(&self.background),
        }
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.quads);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..4);
    }

    /// Renders into a shared texture and waits for the GPU, so the consumer can sample it at once.
    pub fn render_to(&mut self, target: &wgpu::Texture, time: f32) -> Result<(), RenderError> {
        self.update_scene(time);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(target, &mut encoder);
        self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    /// Renders offscreen and copies BGRA pixels to `destination`: the pixel-buffer path.
    pub fn render_to_bytes(
        &mut self,
        time: f32,
        destination: &mut [u8],
        stride: usize,
    ) -> Result<(), RenderError> {
        let row_bytes = (self.width * BYTES_PER_PIXEL) as usize;
        if stride < row_bytes || destination.len() < stride * self.height as usize {
            return Err(RenderError::DestinationTooSmall);
        }
        self.update_scene(time);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(&self.offscreen, &mut encoder);
        let padded = padded_row(self.width);
        encoder.copy_texture_to_buffer(
            self.offscreen.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
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
        let slice = self.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        if let Ok(result) = receiver.recv() {
            result?;
        }
        {
            let mapped = slice.get_mapped_range()?;
            for (source, target) in mapped
                .chunks_exact(padded as usize)
                .zip(destination.chunks_exact_mut(stride))
            {
                target[..row_bytes].copy_from_slice(&source[..row_bytes]);
            }
        }
        self.readback.unmap();
        Ok(())
    }
}

pub fn target_descriptor(width: u32, height: u32) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("spike target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    }
}

fn padded_row(width: u32) -> u32 {
    (width * BYTES_PER_PIXEL).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

/// Three moving overlays and a captions block, as in the ADR-0003 validation scene.
fn scene_quads(time: f32) -> [QuadInstance; 4] {
    let overlay = |phase: f32, color: [f32; 4]| QuadInstance {
        center: [
            0.6 * (time + phase).sin(),
            0.6 * (time * 0.7 + phase * 2.0).cos(),
        ],
        half_size: [0.22, 0.22],
        rotation: time * 0.8 + phase,
        textured: 0.0,
        color,
    };
    [
        overlay(0.0, [0.8, 1.0, 0.2, 0.9]),
        overlay(2.1, [1.0, 0.35, 0.4, 0.85]),
        overlay(4.2, [0.3, 0.75, 1.0, 0.8]),
        QuadInstance {
            center: [0.0, -0.55],
            half_size: [
                0.83,
                0.2 * TEXT_HEIGHT as f32 / TEXT_WIDTH as f32 * 4.5 / 2.0,
            ],
            rotation: 0.0,
            textured: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels_at(renderer: &mut SpikeRenderer, time: f32) -> Vec<u8> {
        let stride = (renderer.width * BYTES_PER_PIXEL) as usize;
        let mut pixels = vec![0; stride * renderer.height as usize];
        renderer.render_to_bytes(time, &mut pixels, stride).unwrap();
        pixels
    }

    fn renderer() -> Option<SpikeRenderer> {
        match SpikeRenderer::new(108, 192) {
            Ok(renderer) => Some(renderer),
            // CI machines may have no GPU; golden tests with a software adapter come later.
            Err(RenderError::NoAdapter(_)) => None,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn scene_is_deterministic_and_animated() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let first = pixels_at(&mut renderer, 1.0);
        assert!(first.iter().step_by(4).any(|blue| *blue > 0));
        assert_eq!(first, pixels_at(&mut renderer, 1.0));
        assert_ne!(first, pixels_at(&mut renderer, 2.0));
    }

    #[test]
    fn small_destination_is_refused() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let result = renderer.render_to_bytes(0.0, &mut [0; 16], 432);
        assert!(matches!(result, Err(RenderError::DestinationTooSmall)));
    }
}
