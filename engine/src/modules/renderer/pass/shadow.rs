use ash::*;
use shipyard::{IntoIter, Unique};
use std::{
	error::Error,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

use crate::modules::renderer::material::{self};

use super::{Pass, RenderTarget, PassID};
use super::command_context::FrameCommand;
use super::frame_sync::FrameSync;
use super::gbuffers::{GBuffers, MAX_SHADOW_CASTERS};
use super::swapchain::Swapchain;
use super::vulkan_context::{Device, RenderFeature};

#[derive(Unique)]
pub(in super::super) struct Shadow {
	// dynamic-rendering mode: one secondary per frame, self-loops begin/end per caster
	commands: Vec<FrameCommand>,
	// render-pass mode: one secondary per (frame, subpass slot). subpass i renders the
	// caster whose `.layer == i` - subpass assignment is fixed at render-pass creation,
	// so it can't follow casters dynamically the way the dynamic-rendering loop does.
	subpass_commands: Vec<Vec<FrameCommand>>,
	// how many of subpass_commands[frame] were actually recorded this frame (active
	// caster count, <= MAX_SHADOW_CASTERS) - read back by buffers() after record().
	recorded_subpasses: Vec<AtomicUsize>,
	render_target: RenderTarget,
	device: Arc<Device>,
}

impl Shadow {
	pub(in super::super) fn new(
		device: Arc<Device>,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let views_per_subpass: Vec<Vec<vk::ImageView>> = (0..MAX_SHADOW_CASTERS)
			.map(|layer| {
				(0..gbuffers.len())
					.map(|i| gbuffers[i].shadow.layer_view(layer))
					.collect()
			})
			.collect();
		let shadow_extent = vk::Extent2D {
			width: gbuffers.shadow_resolution,
			height: gbuffers.shadow_resolution,
		};
		let render_target = RenderTarget::new_multi_subpass(
			&device,
			swapchain,
			shadow_extent,
			gbuffers.shadow_format,
			vk::AttachmentLoadOp::CLEAR,
			vk::AttachmentStoreOp::STORE,
			&views_per_subpass,
		)?;

		let commands = match &device.render_feature {
			RenderFeature::DynamicRendering(_) => (0..swapchain.frame_count)
				.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
				.collect::<Result<Vec<_>, _>>()?,
			RenderFeature::RenderPass => Vec::new(),
		};
		let subpass_commands = match &device.render_feature {
			RenderFeature::RenderPass => (0..swapchain.frame_count)
				.map(|_| {
					(0..MAX_SHADOW_CASTERS)
						.map(|_| FrameCommand::new(device.clone(), vk::CommandBufferLevel::SECONDARY))
						.collect::<Result<Vec<_>, _>>()
				})
				.collect::<Result<Vec<_>, _>>()?,
			RenderFeature::DynamicRendering(_) => Vec::new(),
		};
		let recorded_subpasses = (0..swapchain.frame_count).map(|_| AtomicUsize::new(0)).collect();

		Ok(Self {
			commands,
			subpass_commands,
			recorded_subpasses,
			render_target,
			device,
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

		match &self.render_target {
			RenderTarget::DynamicRendering => {
				let cmd = &self.commands[id];
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
					cmd.device
						.cmd_begin_label(cmd.buffer, "shadow pass record", [1.0, 1.0, 1.0, 1.0]);
					for caster in frame_uniforms.shadow_casters().iter() {
						let shadow_view = gbuffers[id_image].shadow.layer_view(caster.layer);
						if let RenderFeature::DynamicRendering(device) = &cmd.device.render_feature {
							device.cmd_begin_rendering(
								cmd.buffer,
								&vk::RenderingInfo::default()
									.render_area(vk::Rect2D::default().extent(vk::Extent2D {
										width: gbuffers.shadow_resolution,
										height: gbuffers.shadow_resolution,
									}))
									.layer_count(1)
									.depth_attachment(
										&vk::RenderingAttachmentInfo::default()
											.image_view(shadow_view)
											.image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
											.load_op(vk::AttachmentLoadOp::CLEAR)
											.store_op(vk::AttachmentStoreOp::STORE)
											.clear_value(vk::ClearValue {
												depth_stencil: vk::ClearDepthStencilValue {
													depth: 0.0,
													stencil: 0,
												},
											}),
									),
							);
						}
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
								extent: vk::Extent2D {
									width: gbuffers.shadow_resolution,
									height: gbuffers.shadow_resolution,
								},
							}],
						);
						for (mesh, trans, mat_handle) in (mesh_handles, transforms, material_handles).iter() {
							let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else {
								continue;
							};
							let Some(pipeline) = material_manager.get_pipeline(&mat_handle.0, PassID::Shadow, 0) else {
								continue;
							};
							let Some(material) = material_manager.get_material(&mat_handle.0) else {
								continue;
							};

							cmd.device.cmd_bind_pipeline(cmd.buffer, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
							let ctx = material::BindContext {
								transform: trans,
								frame_uniforms,
								light_view_proj: Some(caster.view_proj),
								extent: &swapchain.extent,
								frame_id: id,
							};
							material.bind(frame_sync.frame_id, PassID::Shadow, cmd, pipeline, &ctx);

							cmd.device.cmd_bind_vertex_buffers(cmd.buffer, 0, &[mesh_data.vertex_buffer()], &[0]);
							cmd.device.cmd_bind_index_buffer(cmd.buffer, mesh_data.index_buffer(), 0, vk::IndexType::UINT32);
							cmd.device.cmd_draw_indexed(cmd.buffer, mesh_data.index_count(), 1, 0, 0, 0);
						}

						if let RenderFeature::DynamicRendering(device) = &cmd.device.render_feature {
							device.cmd_end_rendering(cmd.buffer);
						}
					}
					cmd.device.cmd_end_label(cmd.buffer);
					cmd.device.end_command_buffer(cmd.buffer)?;
				}
			},
			RenderTarget::RenderPass { render_pass, subpass_count, .. } => {
				let casters = frame_uniforms.shadow_casters();
				let mut recorded = 0usize;
				for caster in casters.iter() {
					let slot = caster.layer as usize;
					// subpass slot is fixed at render-pass creation time (attachment index),
					// so a caster's layer must land on that same slot every frame - unlike
					// the dynamic-rendering branch, which can point at any view per iteration.
					// buffers() below returns a dense subpass_commands[..recorded] prefix, so
					// casters must additionally arrive in contiguous layer order (0, 1, 2, ...)
					// - a gap here would execute an unrecorded/stale buffer at that slot.
					debug_assert!(slot < *subpass_count as usize, "shadow caster layer out of subpass range");
					debug_assert_eq!(slot, recorded, "shadow caster layers must be contiguous starting at 0");
					let cmd = &self.subpass_commands[id][slot];
					unsafe {
						cmd.device
							.reset_command_pool(cmd.pool, vk::CommandPoolResetFlags::empty())?;
						cmd.device.begin_command_buffer(
							cmd.buffer,
							&vk::CommandBufferBeginInfo::default()
								.flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE)
								.inheritance_info(
									&vk::CommandBufferInheritanceInfo::default()
										.render_pass(*render_pass)
										.subpass(slot as u32),
								),
						)?;
						cmd.device
							.cmd_begin_label(cmd.buffer, "shadow pass record (subpass)", [1.0, 1.0, 1.0, 1.0]);
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
								extent: vk::Extent2D {
									width: gbuffers.shadow_resolution,
									height: gbuffers.shadow_resolution,
								},
							}],
						);
						for (mesh, trans, mat_handle) in (mesh_handles, transforms, material_handles).iter() {
							let Some(mesh_data) = mesh_assets.mesh_assets.get(&mesh.0) else {
								continue;
							};
							let Some(pipeline) = material_manager.get_pipeline(&mat_handle.0, PassID::Shadow, slot as u32) else {
								continue;
							};
							let Some(material) = material_manager.get_material(&mat_handle.0) else {
								continue;
							};

							cmd.device.cmd_bind_pipeline(cmd.buffer, vk::PipelineBindPoint::GRAPHICS, pipeline.pipeline);
							let ctx = material::BindContext {
								transform: trans,
								frame_uniforms,
								light_view_proj: Some(caster.view_proj),
								extent: &swapchain.extent,
								frame_id: id,
							};
							material.bind(frame_sync.frame_id, PassID::Shadow, cmd, pipeline, &ctx);

							cmd.device.cmd_bind_vertex_buffers(cmd.buffer, 0, &[mesh_data.vertex_buffer()], &[0]);
							cmd.device.cmd_bind_index_buffer(cmd.buffer, mesh_data.index_buffer(), 0, vk::IndexType::UINT32);
							cmd.device.cmd_draw_indexed(cmd.buffer, mesh_data.index_count(), 1, 0, 0, 0);
						}
						cmd.device.cmd_end_label(cmd.buffer);
						cmd.device.end_command_buffer(cmd.buffer)?;
					}
					recorded += 1;
				}
				self.recorded_subpasses[id].store(recorded, Ordering::Relaxed);
			},
		}
		Ok(())
	}
}

impl Pass for Shadow {
	fn buffers(&self, frame_sync: &FrameSync) -> Vec<vk::CommandBuffer> {
		let id = frame_sync.frame_id as usize;
		match &self.render_target {
			RenderTarget::DynamicRendering => vec![self.commands[id].buffer],
			RenderTarget::RenderPass { .. } => {
				let recorded = self.recorded_subpasses[id].load(Ordering::Relaxed);
				self.subpass_commands[id][..recorded].iter().map(|c| c.buffer).collect()
			},
		}
	}

	fn render_target(&self) -> &RenderTarget {
		&self.render_target
	}

	fn record(&self, record_view: &super::RecordView) {
		let _ = self.record(record_view);
	}

	fn resize(&mut self, _swapchain: &Swapchain, _gbuffers: &GBuffers) -> Result<(), Box<dyn Error + Send + Sync>> {
		// shadow map resolution is independent of window size - GBuffer::resize only
		// resizes `depth`, never `shadow` - so there's nothing to rebuild here.
		Ok(())
	}
}