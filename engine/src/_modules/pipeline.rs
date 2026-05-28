use std::{error::Error, sync::Arc};
use itertools::Itertools;

use shipyard::*;
use vulkano::{buffer::BufferContents, descriptor_set::{DescriptorBufferInfo, DescriptorImageInfo, DescriptorSet, WriteDescriptorSet, layout::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo, DescriptorType}}, device::Device, image::{ImageLayout, SampleCount}, pipeline::{DynamicState, GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateFlags, PipelineShaderStageCreateInfo, graphics::{GraphicsPipelineCreateInfo, color_blend::{AttachmentBlend, BlendFactor, BlendOp, ColorBlendAttachmentState, ColorBlendState, ColorComponents, LogicOp}, depth_stencil::{CompareOp, DepthState, DepthStencilState}, input_assembly::{InputAssemblyState, PrimitiveTopology}, multisample::MultisampleState, rasterization::{CullMode, DepthBiasState, FrontFace, PolygonMode, RasterizationState}, subpass::PipelineRenderingCreateInfo, vertex_input::{Vertex, VertexDefinition}, viewport::{Scissor, Viewport, ViewportState}}, layout::{PipelineLayoutCreateInfo, PushConstantRange}}, render_pass::Subpass, shader::{EntryPoint, EntryPointInfo, ShaderModule, ShaderModuleCreateInfo, ShaderStages, spirv::ExecutionModel}};
use crate::modules::*;


#[allow(dead_code)]
pub struct Pipeline {
    vertex_shader: Arc<ShaderModule>,
    fragment_shader: Arc<ShaderModule>,
    
    pub pipeline_layout: Arc<PipelineLayout>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub descriptor_set_layout: Arc<DescriptorSetLayout>,
}

impl Pipeline {
    pub fn new(
        vulkan_context: &vulkan_context::VulkanContext,
        vertex_shader: impl AsRef<std::path::Path> + std::marker::Send + 'static,
        fragment_shader: impl AsRef<std::path::Path> + std::marker::Send + 'static,
        num_textures: usize,
        swapchain: &swapchain::Swapchain,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        tracing::debug!("Pipeline::new");

        // Load shaders
        let vertex_shader = Self::create_shader_module(&vulkan_context.device, vertex_shader)?;
        let fragment_shader: Arc<ShaderModule> = Self::create_shader_module(&vulkan_context.device, fragment_shader)?;

        // Create descriptor pool/sets
        let descriptor_set_layout = Self::create_descriptor_set_layout(&vulkan_context, num_textures)?;

        // Create pipeline layout
        let pipeline_layout_info = PipelineLayoutCreateInfo{
            set_layouts: &[&descriptor_set_layout],
            push_constant_ranges: &[],
            ..Default::default()
        };

        let pipeline_layout = PipelineLayout::new(&vulkan_context.device, &pipeline_layout_info)?;

        // Create render_pipelines
        let pipeline = {

            let ve = vertex_shader.entry_point_with_execution("main", ExecutionModel::Vertex).unwrap();
            let fe = fragment_shader.entry_point_with_execution("main", ExecutionModel::Fragment).unwrap();
            let stages = [
                PipelineShaderStageCreateInfo::new(&ve),
                PipelineShaderStageCreateInfo::new(&fe),
            ];

            let vertex_input_state = mesh::Vertex::per_vertex().definition(&ve).unwrap();

            let imput_assembly_state = InputAssemblyState {
                topology: PrimitiveTopology::TriangleList,
                primitive_restart_enable: false,
                ..Default::default()
            };

            GraphicsPipeline::new(
                &vulkan_context.device,
                None,
                &GraphicsPipelineCreateInfo{
                    stages: &stages,
                    vertex_input_state: Some(&vertex_input_state),
                    input_assembly_state: Some(&imput_assembly_state),
                    viewport_state: Some(&ViewportState{
                        viewports: &[Viewport{
                            offset: [0.0, 0.0],
                            extent: [swapchain.extent[0] as f32, swapchain.extent[1] as f32],
                            min_depth: 0.0,
                            max_depth: 1.0,
                        }],
                        scissors: &[Scissor{
                            offset: [0, 0],
                            extent: swapchain.extent
                        }],
                        ..Default::default()
                    }),
                    dynamic_state: &[DynamicState::Viewport, DynamicState::Scissor],
                    rasterization_state: Some(&RasterizationState{
                        depth_clamp_enable: false,
                        rasterizer_discard_enable: false,
                        polygon_mode: PolygonMode::Fill,
                        line_width: 1.0,
                        cull_mode: CullMode::Back,
                        front_face: FrontFace::Clockwise,
                        depth_bias: Some(DepthBiasState{
                            clamp: 0.0,
                            slope_factor: 0.0,
                            constant_factor: 0.0
                        }),
                        ..Default::default()
                    }),
                    multisample_state: Some(&MultisampleState::default()),
                    color_blend_state: Some(&ColorBlendState{
                        attachments: &[ColorBlendAttachmentState{
                            blend: Some(AttachmentBlend{
                                color_blend_op: BlendOp::Add,
                                src_color_blend_factor: BlendFactor::SrcAlpha,
                                dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                                alpha_blend_op: BlendOp::Add,
                                src_alpha_blend_factor: BlendFactor::One,
                                dst_alpha_blend_factor: BlendFactor::Zero
                            }),
                            color_write_mask: ColorComponents::all(),
                            color_write_enable: true,
                        }],
                        logic_op: Some(LogicOp::Copy),
                        ..Default::default()
                    }),
                    depth_stencil_state: Some(&DepthStencilState{
                        depth: Some(DepthState{
                            write_enable: true,
                            compare_op: CompareOp::GreaterOrEqual
                        }),
                        ..Default::default()
                    }),
                    subpass: Some((&PipelineRenderingCreateInfo{
                        color_attachment_formats: &[Some(swapchain.swapchain.image_format())],
                        depth_attachment_format: Some(swapchain.depth_images[0].format()),
                        ..Default::default()
                    }).into()),
                    ..GraphicsPipelineCreateInfo::new(&pipeline_layout)
                }
            )?
        };

        Ok(Self {
            vertex_shader,
            fragment_shader,
            pipeline_layout,
            pipeline,
            descriptor_set_layout,
        })
    }

    pub fn new_with_push<T>(
        vulkan_context: &vulkan_context::VulkanContext,
        vertex_shader: impl AsRef<std::path::Path> + std::marker::Send + 'static,
        fragment_shader: impl AsRef<std::path::Path> + std::marker::Send + 'static,
        num_textures: usize,
        swapchain: &swapchain::Swapchain,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        tracing::debug!("Pipeline::new");

        // Load shaders
        let vertex_shader = Self::create_shader_module(&vulkan_context.device, vertex_shader)?;
        let fragment_shader: Arc<ShaderModule> = Self::create_shader_module(&vulkan_context.device, fragment_shader)?;

        // Create descriptor pool/sets
        let descriptor_set_layout = Self::create_descriptor_set_layout(&vulkan_context, num_textures)?;

        // Push constants for fetching transform
        let push_constant = PushConstantRange{
            offset: 0,
            size: size_of::<T>() as u32,
            stages: ShaderStages::VERTEX,
        };

        // Create pipeline layout
        let pipeline_layout_info = PipelineLayoutCreateInfo{
            set_layouts: &[&descriptor_set_layout],
            push_constant_ranges: &[push_constant],
            ..Default::default()
        };

        let pipeline_layout = PipelineLayout::new(&vulkan_context.device, &pipeline_layout_info)?;

        // Create render_pipelines
        let pipeline = {

            let ve = vertex_shader.entry_point_with_execution("main", ExecutionModel::Vertex).unwrap();
            let fe = fragment_shader.entry_point_with_execution("main", ExecutionModel::Fragment).unwrap();
            let stages = [
                PipelineShaderStageCreateInfo::new(&ve),
                PipelineShaderStageCreateInfo::new(&fe),
            ];

            let vertex_input_state = mesh::Vertex::per_vertex().definition(&ve).unwrap();

            let imput_assembly_state = InputAssemblyState {
                topology: PrimitiveTopology::TriangleList,
                primitive_restart_enable: false,
                ..Default::default()
            };

            GraphicsPipeline::new(
                &vulkan_context.device,
                None,
                &GraphicsPipelineCreateInfo{
                    stages: &stages,
                    vertex_input_state: Some(&vertex_input_state),
                    input_assembly_state: Some(&imput_assembly_state),
                    viewport_state: Some(&ViewportState{
                        viewports: &[Viewport{
                            offset: [0.0, 0.0],
                            extent: [swapchain.extent[0] as f32, swapchain.extent[1] as f32],
                            min_depth: 0.0,
                            max_depth: 1.0,
                        }],
                        scissors: &[Scissor{
                            offset: [0, 0],
                            extent: swapchain.extent
                        }],
                        ..Default::default()
                    }),
                    dynamic_state: &[DynamicState::Viewport, DynamicState::Scissor],
                    rasterization_state: Some(&RasterizationState{
                        depth_clamp_enable: false,
                        rasterizer_discard_enable: false,
                        polygon_mode: PolygonMode::Fill,
                        line_width: 1.0,
                        cull_mode: CullMode::Back,
                        front_face: FrontFace::Clockwise,
                        depth_bias: Some(DepthBiasState{
                            clamp: 0.0,
                            slope_factor: 0.0,
                            constant_factor: 0.0
                        }),
                        ..Default::default()
                    }),
                    multisample_state: Some(&MultisampleState::default()),
                    color_blend_state: Some(&ColorBlendState{
                        attachments: &[ColorBlendAttachmentState{
                            blend: Some(AttachmentBlend{
                                color_blend_op: BlendOp::Add,
                                src_color_blend_factor: BlendFactor::SrcAlpha,
                                dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                                alpha_blend_op: BlendOp::Add,
                                src_alpha_blend_factor: BlendFactor::One,
                                dst_alpha_blend_factor: BlendFactor::Zero
                            }),
                            color_write_mask: ColorComponents::all(),
                            color_write_enable: true,
                        }],
                        logic_op: Some(LogicOp::Copy),
                        ..Default::default()
                    }),
                    depth_stencil_state: Some(&DepthStencilState{
                        depth: Some(DepthState{
                            write_enable: true,
                            compare_op: CompareOp::GreaterOrEqual
                        }),
                        ..Default::default()
                    }),
                    subpass: Some((&PipelineRenderingCreateInfo{
                        color_attachment_formats: &[Some(swapchain.swapchain.image_format())],
                        depth_attachment_format: Some(swapchain.depth_images[0].format()),
                        ..Default::default()
                    }).into()),
                    ..GraphicsPipelineCreateInfo::new(&pipeline_layout)
                }
            )?
        };

        Ok(Self {
            vertex_shader,
            fragment_shader,
            pipeline_layout,
            pipeline,
            descriptor_set_layout,
        })
    }

    fn create_shader_module<P: AsRef<std::path::Path>>(
        device: &Arc<Device>,
        path: P,
    ) -> Result<Arc<ShaderModule>, Box<dyn Error + Send + Sync>> {
        let bytes = std::fs::read(path)?;
        let words = unsafe {
            std::slice::from_raw_parts(
                bytes.as_ptr() as * const u32,
                bytes.len() / 4
            )
        };
        let shader_module = unsafe {
            ShaderModule::new(
                &device,
                &ShaderModuleCreateInfo::new(&words)
            )?
        };

        Ok(shader_module)
    }

    fn create_descriptor_set_layout(
        vulkan_context: &vulkan_context::VulkanContext,
        num_textures: usize,
    ) -> Result<Arc<DescriptorSetLayout>, Box<dyn Error + Send + Sync>> {
        let ub_layout_binding = DescriptorSetLayoutBinding {
            binding: 0,
            stages: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
            ..DescriptorSetLayoutBinding::new(DescriptorType::UniformBuffer)
        };

		let mut bindings = vec![ub_layout_binding];
		
		if num_textures > 0 {
			bindings.push(DescriptorSetLayoutBinding {
				binding: 1,
				stages: ShaderStages::FRAGMENT,
				descriptor_count: num_textures as u32,
				..DescriptorSetLayoutBinding::new(DescriptorType::CombinedImageSampler)
			});
		}
		
		tracing::debug!("bindings: {bindings:?}");
        let layout_info = DescriptorSetLayoutCreateInfo{
            bindings: &bindings,
            ..Default::default()
        };
        let layout = DescriptorSetLayout::new(&vulkan_context.device, &layout_info)?;

        Ok(layout)
    }
}