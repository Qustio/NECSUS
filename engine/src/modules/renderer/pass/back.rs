use ash::*;
use shipyard::Unique;
use std::{error::Error, sync::Arc};

use super::Pass;
use super::buffer::Buffer;
use super::command_context::FrameCommand;
use super::frame_sync::FrameSync;
use super::swapchain::Swapchain;
use super::vulkan_context::{Allocator, Device};

#[derive(Unique)]
pub(in super::super) struct Back {
	commands: Vec<FrameCommand>,
	pipeline: Pipeline,
	vertexes: Buffer<nalgebra_glm::Vec2>,
	indexes: Buffer<u32>,
	image_format: vk::Format,
}

impl Back {
	pub(in super::super) fn new(
		device: Arc<Device>,
		allocator: Arc<Allocator>,
		frame_count: u32,
		image_format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let commands = (0..frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
			.collect::<Result<Vec<_>, _>>()?;
		let pipeline = Pipeline::new(device, image_format)?;
		let vertexes = Buffer::from_slice(
			allocator.clone(),
			&[
				nalgebra_glm::Vec2::new(-1.0, -1.0),
				nalgebra_glm::Vec2::new(-1.0, 1.0),
				nalgebra_glm::Vec2::new(1.0, -1.0),
				nalgebra_glm::Vec2::new(1.0, 1.0),
			],
			vk::BufferUsageFlags::VERTEX_BUFFER,
		)?;
		let indexes = Buffer::from_slice(
			allocator.clone(),
			&[0, 1, 3, 0, 3, 2],
			vk::BufferUsageFlags::INDEX_BUFFER,
		)?;
		Ok(Self {
			commands,
			pipeline,
			vertexes,
			indexes,
			image_format,
		})
	}

	pub(in super::super) fn record(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		unsafe {
			// reset all buffers in pool
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;
			cmd.device.begin_command_buffer(
				cmd.buffer,
				&vk::CommandBufferBeginInfo::default()
					.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
					.inheritance_info(
						&vk::CommandBufferInheritanceInfo::default().push_next(
							&mut vk::CommandBufferInheritanceRenderingInfo::default()
								.color_attachment_formats(&[self.image_format])
								.rasterization_samples(vk::SampleCountFlags::TYPE_1),
						),
					),
			)?;
			cmd.device.cmd_begin_rendering(
				cmd.buffer,
				&vk::RenderingInfo::default()
					.render_area(vk::Rect2D::default().extent(swapchain.extent))
					.layer_count(1)
					.color_attachments(&[vk::RenderingAttachmentInfo::default()
						.image_view(image_view)
						.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
						.load_op(vk::AttachmentLoadOp::LOAD)
						.store_op(vk::AttachmentStoreOp::STORE)]),
			);
			cmd.device.cmd_set_viewport(
				cmd.buffer,
				0,
				&[vk::Viewport {
					x: 0.0,
					y: 0.0,
					width: swapchain.extent.width as f32,
					height: swapchain.extent.height as f32,
					min_depth: 0.0,
					max_depth: 1.0,
				}],
			);
			cmd.device.cmd_set_scissor(
				cmd.buffer,
				0,
				&[vk::Rect2D {
					offset: vk::Offset2D { x: 0, y: 0 },
					extent: swapchain.extent,
				}],
			);
			cmd.device.cmd_bind_pipeline(
				cmd.buffer,
				vk::PipelineBindPoint::GRAPHICS,
				self.pipeline.pipeline(),
			);
			cmd.device
				.cmd_bind_vertex_buffers(cmd.buffer, 0, &[self.vertexes.buffer()], &[0]);
			cmd.device.cmd_bind_index_buffer(
				cmd.buffer,
				self.indexes.buffer(),
				0,
				vk::IndexType::UINT32,
			);
			cmd.device.cmd_draw_indexed(cmd.buffer, 6, 1, 0, 0, 0);
			cmd.device.cmd_end_rendering(cmd.buffer);
			cmd.device.end_command_buffer(cmd.buffer)?;
		}
		Ok(())
	}
}

impl Pass for Back {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}

pub struct Pipeline {
	pipeline: vk::Pipeline,
	layout: vk::PipelineLayout,
	shader: vk::ShaderModule,
	device: Arc<Device>,
}

impl Pipeline {
	pub(super) fn new(
		device: Arc<Device>,
		image_format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let push_range = vk::PushConstantRange::default()
			.stage_flags(vk::ShaderStageFlags::VERTEX)
			.offset(0)
			.size(64); // mat4
		let layout = unsafe {
			device.create_pipeline_layout(
				&vk::PipelineLayoutCreateInfo::default()
					.set_layouts(&[])
					.push_constant_ranges(&[push_range]),
				None,
			)?
		};

		let shader = unsafe {
			let exe = std::env::current_exe()?;
			let mut file =
				std::fs::File::open(exe.parent().unwrap().join("shaders").join("back.slang"))?;
			let spv = util::read_spv(&mut file)?;
			device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spv), None)?
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

			let attrs = [vk::VertexInputAttributeDescription::default()
				.location(0)
				.binding(0)
				.format(vk::Format::R32G32_SFLOAT)
				.offset(0)];

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

			let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
			let dynamic_state =
				vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

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
				device
					.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
					.map_err(|(_, e)| e)?[0]
			}
		};

		Ok(Self {
			pipeline,
			layout,
			shader,
			device,
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
