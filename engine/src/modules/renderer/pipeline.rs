use std::{error::Error, sync::Arc};

use ash::*;
use super::vulkan_context::Device;

pub struct Pipeline {
	pipeline: vk::Pipeline,
	layout: vk::PipelineLayout,
	shader: vk::ShaderModule,
	device: Arc<Device>
}

impl Pipeline {
	pub(super) fn new(
		device: Arc<Device>,
		image_format: vk::Format,
	)  -> Result<Self, Box<dyn Error + Send + Sync>> {
		let push_range = vk::PushConstantRange::default()
			.stage_flags(vk::ShaderStageFlags::VERTEX)
			.offset(0)
			.size(64); // mat4
		let layout = unsafe {
			device.create_pipeline_layout(
				&vk::PipelineLayoutCreateInfo::default()
					.set_layouts(&[])
					.push_constant_ranges(&[push_range]),
				None
			)?
		};

		let shader = unsafe {
			let mut file = std::fs::File::open("engine/src/modules/renderer/shaders/shader.bin")?;
			let spv = util::read_spv(&mut file)?;
			device.create_shader_module(
				&vk::ShaderModuleCreateInfo::default()
					.code(&spv),
				None
			)?
		};

		let pipeline = {
			let stages = [
				vk::PipelineShaderStageCreateInfo::default()
					.stage(vk::ShaderStageFlags::VERTEX)
					.module(shader)
					.name(c"vertMain"),
				vk::PipelineShaderStageCreateInfo::default()
					.stage(vk::ShaderStageFlags::FRAGMENT)
					.module(shader)
					.name(c"fragMain"),
			];

			let binding = vk::VertexInputBindingDescription::default()
				.binding(0)
				.stride(8)
				.input_rate(vk::VertexInputRate::VERTEX);

			let bindings = [binding];

			let attrs = [
				vk::VertexInputAttributeDescription::default().location(0).binding(0).format(vk::Format::R32G32_SFLOAT).offset(0),
			];

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
				.blend_enable(false)
				.color_write_mask(vk::ColorComponentFlags::RGBA);

			let attachments = [blend_attachment];

			let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
				.attachments(&attachments);

			let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
			let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
				.dynamic_states(&dynamic_states);

			let attachment_formats = [image_format];

			let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
				.color_attachment_formats(&attachment_formats);

			let info = vk::GraphicsPipelineCreateInfo::default()
				.stages(&stages)
				.vertex_input_state(&vertex_input)
				.input_assembly_state(&input_assembly)
				.viewport_state(&viewport_state)
				.rasterization_state(&rasterization)
				.multisample_state(&multisample)
				.color_blend_state(&color_blend)
				.dynamic_state(&dynamic_state)
				.layout(layout)
				.push_next(&mut rendering_info);
			
			unsafe {
				device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
					.map_err(|(_, e)| e)?[0]
			}
		};
		
		Ok(Self{
			pipeline,
			layout,
			shader,
			device
		})
	}

	pub(super) fn pipeline(&self) -> vk::Pipeline {
		self.pipeline
	}
}

impl Drop for Pipeline {
	fn drop(&mut self) {
		unsafe {
			self.device.destroy_shader_module(self.shader, None);
			self.device.destroy_pipeline_layout(self.layout, None);
			self.device.destroy_pipeline(self.pipeline, None);
		}
	}
}