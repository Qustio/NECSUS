use bytemuck::bytes_of;
use nalgebra_glm::Mat4;

use crate::modules::{components, renderer::{gbuffers::GBuffers, mesh::{Vertex, VertexDescription}, swapchain::Swapchain}};

use super::*;

pub struct StandartMaterial {
    shader: vk::ShaderModule,
    color_format: vk::Format,
    depth_format: vk::Format,
    device: Arc<Device>
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DrawConstants {
	model: Mat4,
	view: Mat4,
	proj: Mat4,
}

impl DrawConstants {
	pub(crate) fn new(
		extent: &vk::Extent2D,
		camera: &components::Camera,
		transform: &components::Transform,
	) -> Self {
		let aspect = extent.width as f32 / extent.height as f32;
		let mut proj = nalgebra_glm::perspective_rh_zo(aspect, 90_f32.to_radians(), 1000.0, 0.001);
		let rev_z_matrix = Mat4::new(
			1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
		);
		proj *= rev_z_matrix;
		let view = camera.view_matrix();
		let model = transform.local;
		Self { model, view, proj }
	}
}

impl StandartMaterial {
	pub fn new(
        device: Arc<Device>,
        swapchain: &Swapchain,
        gbuffers: &GBuffers,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let shader = unsafe {
			let blob = super::build_shader("engine/shaders/main.slang", &["vertMain", "fragMain"])?;
			let spv: &[u32] = bytemuck::cast_slice(blob.as_slice());
			device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spv), None)?
		};
		Ok(Self {
            shader,
            color_format: swapchain.format.format,
            depth_format: gbuffers.depth_format,
            device
        })
	}
    
    fn create_pipelines(
        &self
    ) -> Result<Vec<Pipeline>, Box<dyn Error + Send + Sync>> {
        let push_range = vk::PushConstantRange::default()
			.stage_flags(vk::ShaderStageFlags::VERTEX)
			.offset(0)
			.size(size_of::<DrawConstants>() as u32); // mat4
        let layout = unsafe {
			self.device.create_pipeline_layout(
				&vk::PipelineLayoutCreateInfo::default()
					.set_layouts(&[])
					.push_constant_ranges(&[push_range]),
				None,
			)?
		};
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(self.shader)
                .name(c"vertMain"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(self.shader)
                .name(c"fragMain"),
        ];

        let binding = Vertex::binding();

        let bindings = [binding];

        let attrs = Vertex::attributes();

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attrs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);

        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD);

        let attachments = [blend_attachment];

        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);

        let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let attachment_formats = [self.color_format];

        let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&attachment_formats)
            .depth_attachment_format(vk::Format::D32_SFLOAT);

        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .depth_stencil_state(&depth_state)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .push_next(&mut rendering_info);
        
        let infos = [info];
        let pipeline = unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &infos, None)
                .map_err(|(_, e)| e)?
        };
        let pipelines = pipeline.iter().zip(infos).map(|(pipeline, i)| {
            Pipeline{
                pipeline: *pipeline,
                layout: i.layout,
                device: self.device.clone(),
            }
        }).collect::<Vec<_>>();
        Ok(pipelines)
    }
}

impl Drop for StandartMaterial {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader, None);
        }
    }
}

impl Materal for StandartMaterial {
    fn create_pipeline(&self) -> Result<Vec<(PassID, Pipeline)>, Box<dyn Error + Send + Sync>> {
        let mut pipelines = self.create_pipelines()?;
        return Ok(vec![
            (PassID::Geometry, pipelines.remove(0))
        ])
    }
    
    fn bind(&self, cmd: &FrameCommand, pipeline: &Pipeline, ctx: &BindContext) {
        unsafe {
            let constant = DrawConstants::new(ctx.extent, ctx.camera, ctx.transform);
            cmd.device.cmd_push_constants(
                cmd.buffer,
                pipeline.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytes_of(&constant),
            );
        }
    }
    
}