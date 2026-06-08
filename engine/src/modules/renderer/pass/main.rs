use ash::*;
use bytemuck::bytes_of;
use shipyard::{IntoIter, Unique, View};
use std::{error::Error, sync::Arc};

use crate::modules::renderer::material;
use crate::modules::renderer::pass::PassID;

use super::Pass;
use super::command_context::FrameCommand;
use super::components;
use super::frame_sync::FrameSync;
use super::gbuffers::GBuffers;
use super::mesh;
use super::swapchain::Swapchain;
use super::vulkan_context::Device;

#[derive(Unique)]
pub(in super::super) struct Main {
	commands: Vec<FrameCommand>,
}

impl Main {
	pub(in super::super) fn new(
		device: Arc<Device>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let commands = (0..frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			commands,
		})
	}

	pub(in super::super) fn record(
		&self,
		frame_sync: &FrameSync,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
		mesh_assets: &mesh::MeshAssetManager,
		mesh_handles: &View<mesh::MeshHandle>,
		material_manager: &material::MaterialManager,
		material_handles: &View<material::MaterialHandle>,
		transforms: &View<components::Transform>,
		camera: &components::Camera,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		let depth_view = gbuffers[id_image].depth.view;
		unsafe {
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;
			cmd.device.begin_command_buffer(
				cmd.buffer,
				&vk::CommandBufferBeginInfo::default()
					.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
					.inheritance_info(
						&vk::CommandBufferInheritanceInfo::default().push_next(
							&mut vk::CommandBufferInheritanceRenderingInfo::default()
								.color_attachment_formats(&[swapchain.format.format])
								.depth_attachment_format(gbuffers.depth_format)
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
						.store_op(vk::AttachmentStoreOp::STORE)])
					.depth_attachment(&vk::RenderingAttachmentInfo::default()
						.image_view(depth_view)
						.image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
						.load_op(vk::AttachmentLoadOp::LOAD)
						.store_op(vk::AttachmentStoreOp::STORE)
					),
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
			for (mesh, trans, mat_handle) in (mesh_handles, transforms, material_handles).iter() {
				let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else { continue };
				let Some(pipeline) = material_manager.get_pipeline(&mat_handle.0, PassID::Geometry) else { continue };
				let Some(material) = material_manager.get_material(&mat_handle.0) else { continue };

				cmd.device.cmd_bind_pipeline(
					cmd.buffer,
					vk::PipelineBindPoint::GRAPHICS,
					pipeline.pipeline,
				);
				let ctx = material::BindContext {
					transform: trans,
					camera,
					extent: &swapchain.extent,
					frame_id: id,
				};
				material.bind(cmd, pipeline, &ctx);

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
			cmd.device.end_command_buffer(cmd.buffer)?;
		}
		Ok(())
	}
}

impl Pass for Main {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}