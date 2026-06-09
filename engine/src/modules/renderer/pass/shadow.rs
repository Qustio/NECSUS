use ash::*;
use bytemuck::bytes_of;
use shipyard::{IntoIter, Unique, View};
use std::{error::Error, sync::Arc};

use crate::modules::renderer::material::{self, Pipeline, build_shader};
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
pub(in super::super) struct Shadow {
	commands: Vec<FrameCommand>,
}

impl Shadow {
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
		light: &components::DirectionalLight,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let shadow_view = gbuffers[id_image].shadow.view;
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
								.depth_attachment_format(gbuffers.shadow_format)
								.rasterization_samples(vk::SampleCountFlags::TYPE_1),
						),
					),
			)?;
			// need to be for light component from here
			cmd.device.cmd_begin_rendering(
				cmd.buffer,
				&vk::RenderingInfo::default()
					.render_area(vk::Rect2D::default().extent(vk::Extent2D{
						width: gbuffers.shadow_resolution,
						height: gbuffers.shadow_resolution,
					}))
					.layer_count(1)
					.depth_attachment(&vk::RenderingAttachmentInfo::default()
						.image_view(shadow_view)
						.image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
						.load_op(vk::AttachmentLoadOp::CLEAR)
						.store_op(vk::AttachmentStoreOp::STORE)
						.clear_value(vk::ClearValue {
							depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 },
						})
					)
			);
			cmd.device.cmd_set_viewport(
				cmd.buffer,
				0,
				&[vk::Viewport {
					x: 0.0,
					y: 0.0,
					width: gbuffers.shadow_resolution as f32,
					height: gbuffers.shadow_resolution as f32,
					min_depth: 0.0,
					max_depth: 1.0,
				}],
			);
			cmd.device.cmd_set_scissor(
				cmd.buffer,
				0,
				&[vk::Rect2D {
					offset: vk::Offset2D { x: 0, y: 0 },
					extent: vk::Extent2D{
						width: gbuffers.shadow_resolution,
						height: gbuffers.shadow_resolution,
					},
				}],
			);
			for (mesh, trans, mat_handle) in (mesh_handles, transforms, material_handles).iter() {
				let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else { continue };
				let Some(pipeline) = material_manager.get_pipeline(&mat_handle.0, PassID::Shadow) else { continue };
				let Some(material) = material_manager.get_material(&mat_handle.0) else { continue };

				cmd.device.cmd_bind_pipeline(
					cmd.buffer,
					vk::PipelineBindPoint::GRAPHICS,
					pipeline.pipeline,
				);
				let ctx = material::BindContext {
					transform: trans,
					camera: None,
					extent: &swapchain.extent,
					frame_id: id,
					light: Some(&light)
				};
				material.bind(PassID::Shadow, cmd, pipeline, &ctx);

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

impl Pass for Shadow {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		let id = frame_sync.frame_id as usize;
		self.commands[id].buffer
	}
}