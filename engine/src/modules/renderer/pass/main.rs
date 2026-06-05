use ash::*;
use bytemuck::bytes_of;
use nalgebra_glm::Mat4;
use shipyard::{IntoIter, Unique, View};
use std::{error::Error, sync::Arc};

use super::Pass;
use super::command_context::FrameCommand;
use super::components;
use super::frame_sync::FrameSync;
use super::mesh;
use super::mesh::Vertex;
use super::mesh::VertexDescription;
use super::swapchain::Swapchain;
use super::vulkan_context::Device;

#[derive(Unique)]
pub(in super::super) struct Main {
	commands: Vec<FrameCommand>,
	pipeline: Pipeline,
	image_format: vk::Format,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawConstants {
	model: Mat4,
	view: Mat4,
	proj: Mat4,
}

impl DrawConstants {
	fn new(
		extent: &vk::Extent2D,
		camera: &components::Camera,
		transform: &components::Transform,
	) -> Self {
		let aspect = extent.width as f32 / extent.height as f32;
		let mut proj = nalgebra_glm::perspective_rh_zo(aspect, 90_f32.to_radians(), 0.1, 100.0);
		let rev_z_matrix = Mat4::new(
			1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
		);
		proj *= rev_z_matrix;
		let view = camera.view_matrix();
		let model = transform.local;
		Self { model, view, proj }
	}
}

impl Main {
	pub(in super::super) fn new(
		device: Arc<Device>,
		frame_count: u32,
		image_format: vk::Format,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let commands = (0..frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
			.collect::<Result<Vec<_>, _>>()?;
		let pipeline = Pipeline::new(device, image_format)?;
		Ok(Self {
			commands,
			pipeline,
			image_format,
		})
	}

	pub(in super::super) fn record(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
		mesh_assets: &mesh::MeshAssetManager,
		mesh_handles: &View<mesh::MeshHandle>,
		transforms: &View<components::Transform>,
		camera: &components::Camera,
	) {
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		unsafe {
			// reset all buffers in pool
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty());
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
			);
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

			// meshes
			for (mesh, trans) in (mesh_handles, transforms).iter() {
				let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else {
					continue;
				};
				let constant = DrawConstants::new(&swapchain.extent, camera, trans);
				cmd.device.cmd_push_constants(
					cmd.buffer,
					self.pipeline.layout,
					vk::ShaderStageFlags::VERTEX,
					0,
					bytes_of(&constant),
				);
				cmd.device.cmd_bind_vertex_buffers(
					cmd.buffer,
					0,
					&[mesh_data.vertex_buffer()],
					&[0],
				);
				cmd.device.cmd_bind_index_buffer(
					cmd.buffer,
					mesh_data.index_buffer(),
					0,
					vk::IndexType::UINT32,
				);
				cmd.device
					.cmd_draw_indexed(cmd.buffer, mesh_data.index_count(), 1, 0, 0, 0);
			}

			cmd.device.cmd_end_rendering(cmd.buffer);
			cmd.device.end_command_buffer(cmd.buffer);
		}
	}
}

impl Pass for Main {
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
			.size(size_of::<DrawConstants>() as u32); // mat4
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
				std::fs::File::open(exe.parent().unwrap().join("shaders").join("main.slang"))?;
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
