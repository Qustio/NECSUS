use ash::*;
use bytemuck::bytes_of;
use shipyard::{IntoIter, Unique, View};
use std::{error::Error, sync::Arc};

use crate::modules::renderer::material;

use super::{Pass, RenderTarget, AttachmentType, PassID};
use super::command_context::FrameCommand;
use super::components;
use super::frame_sync::FrameSync;
use super::gbuffers::GBuffers;
use super::mesh;
use super::swapchain::Swapchain;
use super::vulkan_context::{Device, RenderFeature};
#[derive(Unique)]
pub(in super::super) struct Main {
	commands: Vec<FrameCommand>,
	render_target: RenderTarget,
	device: Arc<Device>
}

impl Main {
	pub(in super::super) fn new(
		device: Arc<Device>,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let commands = (0..swapchain.frame_count)
			.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
			.collect::<Result<Vec<_>, _>>()?;
		let depth_views: Vec<vk::ImageView> = (0..gbuffers.len()).map(|i| gbuffers[i].depth.view).collect();
		let render_target = RenderTarget::new(
			&device,
			&swapchain,
			&[
				(AttachmentType::Image, swapchain.format.format, &swapchain.image_views,  vk::AttachmentLoadOp::CLEAR, vk::AttachmentStoreOp::STORE),
				(AttachmentType::Depth, gbuffers.depth_format, &depth_views,  vk::AttachmentLoadOp::CLEAR, vk::AttachmentStoreOp::STORE),
			],
		)?;
		Ok(Self {
			commands,
			render_target,
			device
		})
	}

	pub(in super::super) fn record(
		&self,
		record_view: &super::RecordView,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		// extract views from record_view
		let frame_sync = &record_view.frame_sync;
		let swapchain = &record_view.swapchain;
		let gbuffers = &record_view.gbuffers;
		let mesh_assets = &record_view.mesh_assets;
		let mesh_handles = &record_view.mesh_handles;
		let material_manager = &record_view.material_manager;
		let material_handles = &record_view.material_handles;
		let transforms = &record_view.transforms;
		let frame_uniforms = &record_view.frame_uniforms;

		let id = frame_sync.frame_id as usize;
		let id_image = frame_sync.acquired_image_index as usize;
		let cmd = &self.commands[id];
		let image_view = swapchain.image_views[id_image];
		let depth_view = gbuffers[id_image].depth.view;
		unsafe {
			cmd.device
				.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;

			match &self.render_target {
				RenderTarget::RenderPass { render_pass, .. } => {
					cmd.device.begin_command_buffer(
						cmd.buffer,
						&vk::CommandBufferBeginInfo::default()
							.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE)
							.inheritance_info(
								&vk::CommandBufferInheritanceInfo::default()
									.render_pass(*render_pass)
									.subpass(0)
							),
					)?;
				},
				RenderTarget::DynamicRendering => {
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
				},
			}
			cmd.device
				.cmd_begin_label(cmd.buffer, "main pass record", [1.0, 1.0, 1.0, 1.0]);

			if let RenderTarget::DynamicRendering { .. } = &self.render_target {
				if let RenderFeature::DynamicRendering(device) = &cmd.device.render_feature {
					device.cmd_begin_rendering(
						cmd.buffer,
						&vk::RenderingInfo::default()
							.render_area(vk::Rect2D::default().extent(swapchain.extent))
							.layer_count(1)
							.color_attachments(&[vk::RenderingAttachmentInfo::default()
								.image_view(image_view)
								.image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
								.load_op(vk::AttachmentLoadOp::LOAD)
								.store_op(vk::AttachmentStoreOp::STORE)])
							.depth_attachment(
								&vk::RenderingAttachmentInfo::default()
									.image_view(depth_view)
									.image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
									.load_op(vk::AttachmentLoadOp::LOAD)
									.store_op(vk::AttachmentStoreOp::STORE),
							),
					);
				}
			}
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
				let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else {
					continue;
				};
				let Some(pipeline) = material_manager.get_pipeline(&mat_handle.0, PassID::Geometry, 0)
				else {
					continue;
				};
				let Some(material) = material_manager.get_material(&mat_handle.0) else {
					continue;
				};

				cmd.device.cmd_bind_pipeline(
					cmd.buffer,
					vk::PipelineBindPoint::GRAPHICS,
					pipeline.pipeline,
				);
				let ctx = material::BindContext {
					transform: trans,
					frame_uniforms,
					light_view_proj: None,
					extent: &swapchain.extent,
					frame_id: id,
				};
				material.bind(frame_sync.frame_id, PassID::Geometry, cmd, pipeline, &ctx);

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
			if let RenderTarget::DynamicRendering { .. } = &self.render_target {
				if let RenderFeature::DynamicRendering(device) = &cmd.device.render_feature {
					device.cmd_end_rendering(cmd.buffer);
				}
			}
			cmd.device.cmd_end_label(cmd.buffer);
			cmd.device.end_command_buffer(cmd.buffer)?;
		}
		Ok(())
	}
}

impl Pass for Main {
	fn buffers(&self, frame_sync: &FrameSync) -> Vec<vk::CommandBuffer> {
		let id = frame_sync.frame_id as usize;
		vec![self.commands[id].buffer]
	}
	
	fn render_target(&self) -> &RenderTarget {
		&self.render_target
	}

	fn record(&self, record_view: &super::RecordView) {
		self.record(record_view);
	}
	
	fn resize(&mut self, swapchain: &Swapchain, gbuffers: &GBuffers) -> Result<(), Box<dyn Error + Send + Sync>> {
		let depth_views: Vec<vk::ImageView> = (0..gbuffers.len()).map(|i| gbuffers[i].depth.view).collect();
		self.render_target.resize(&self.device, swapchain, &[
			(AttachmentType::Image, swapchain.format.format, &swapchain.image_views,  vk::AttachmentLoadOp::CLEAR, vk::AttachmentStoreOp::STORE),
			(AttachmentType::Depth, gbuffers.depth_format, &depth_views,  vk::AttachmentLoadOp::CLEAR, vk::AttachmentStoreOp::STORE),
		])
	}
}
