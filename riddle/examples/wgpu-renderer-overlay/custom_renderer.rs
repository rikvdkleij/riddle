use riddle::{platform::*, renderer::*};

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use std::cell::RefCell;
use wgpu::util::DeviceExt;

pub struct CustomRenderer {
    _adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    _surface: wgpu::Surface,
    swap_chain: wgpu::SwapChain,

    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,

    pub current_frame: Option<wgpu::SwapChainFrame>,
    current_encoder: Option<RefCell<wgpu::CommandEncoder>>,
}

impl CustomRenderer {
    pub fn new(window: &Window) -> Result<CustomRenderer> {
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };

        let adapter =
            futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: Some(&surface),
            }))
            .ok_or(RendererError::APIInitError("Failed to get WGPU adapter"))?;

        let (device, queue) = futures::executor::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                shader_validation: true,
                ..Default::default()
            },
            None,
        ))
        .map_err(|_| RendererError::APIInitError("Failed to create WGPU device"))?;

        let (width, height) = window.physical_size();
        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width,
            height,
            present_mode: wgpu::PresentMode::Mailbox,
        };

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let vs_bytes = include_bytes!("shaders/default.vert.spv");
        let mut vs_vec = vec![0u8; vs_bytes.len()];
        vs_vec.copy_from_slice(&vs_bytes[..]);
        let vs_module = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            std::borrow::Cow::from(bytemuck::cast_slice(&vs_vec[..])),
        ));

        let fs_bytes = include_bytes!("shaders/default.frag.spv");
        let mut fs_vec = vec![0u8; fs_bytes.len()];
        fs_vec.copy_from_slice(&fs_bytes[..]);
        let fs_module = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            std::borrow::Cow::from(bytemuck::cast_slice(&fs_vec[..])),
        ));

        let vertex_size = std::mem::size_of::<Vertex>();

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
            label: None,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &fs_module,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::None,
                ..Default::default()
            }),
            primitive_topology: wgpu::PrimitiveTopology::PointList,
            color_states: &[wgpu::ColorStateDescriptor {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                color_blend: wgpu::BlendDescriptor {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha_blend: wgpu::BlendDescriptor {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[wgpu::VertexBufferDescriptor {
                    stride: vertex_size as wgpu::BufferAddress,
                    step_mode: wgpu::InputStepMode::Vertex,
                    attributes: &[wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        Ok(Self {
            _adapter: adapter,
            _surface: surface,
            device,
            queue,
            swap_chain,

            bind_group_layout,
            pipeline: render_pipeline,

            current_frame: None,
            current_encoder: None,
        })
    }

    pub fn begin(&mut self) -> Result<()> {
        let new_frame = self.swap_chain.get_current_frame()?;
        let mut new_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let encoder = &mut new_encoder;
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &new_frame.output.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        self.current_frame = Some(new_frame);
        self.current_encoder = Some(RefCell::new(new_encoder));

        Ok(())
    }

    pub fn draw_points(&self, points: &Vec<Vertex>, distance: f32, rotation: f32) {
        let proj_matrix =
            glam::Mat4::perspective_rh(std::f32::consts::PI / 1.5, 4.0 / 3.0, 0.0, 100.0);
        let view_matrix = glam::Mat4::from_translation(glam::vec3(0.0, 0.0, distance));
        let rot = glam::Mat4::from_rotation_y(rotation);
        let result_matrix = proj_matrix * view_matrix * rot;
        //let result_matrix = glam::Mat4::identity();
        let matrix_arr: &[f32; 16] = result_matrix.as_ref();

        let camera_uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(matrix_arr),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(camera_uniform.slice(..)),
            }],
            label: None,
        });

        let vertex_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&points[..]),
                usage: wgpu::BufferUsage::VERTEX,
            });

        let mut encoder = self.current_encoder.as_ref().unwrap().borrow_mut();
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.current_frame.as_ref().unwrap().output.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });
        rpass.set_pipeline(&self.pipeline);

        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.set_vertex_buffer(0, vertex_buf.slice(..));
        rpass.draw(0..(points.len() as u32), 0..1);
        drop(rpass);
    }

    pub fn commit(&mut self) {
        let encoder = self.current_encoder.take().unwrap().into_inner();
        let cmd = encoder.finish();
        self.queue.submit(Some(cmd));

        let new_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.current_encoder = Some(RefCell::new(new_encoder));
    }

    pub fn end(&mut self) {
        let encoder = self.current_encoder.take().unwrap().into_inner();
        let cmd = encoder.finish();
        self.queue.submit(Some(cmd));
        self.current_frame = None;
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 3],
}

unsafe impl Pod for Vertex {}
unsafe impl Zeroable for Vertex {}
