use std::ops::DerefMut;
use std::{ops::Deref, sync::Arc};

use ash::vk;
use hashbrown::HashMap;
use shipyard::{Unique, UniqueView};


use crate::modules::renderer::vulkan_context::Allocator;
use super::vulkan_context::{RenderFeature, Device};
use super::swapchain::Swapchain;
use super::gbuffers::GBuffers;

use super::frame_sync::FrameSync;

//pub mod back;
pub mod main;
pub mod shadow;

pub use super::*;

#[derive(Unique)]
pub struct PassManager {
	passes: HashMap<PassID, Box<dyn Pass>>,
	//allocator: Arc<Allocator>,
}

impl PassManager {
	pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
		Ok(Self{
			passes: HashMap::default()
		})
	}
	pub fn insert(
		&mut self,
		pass_id: PassID,
		pass: Box<dyn Pass>
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		match self.passes.insert(pass_id, pass) {
			Some(_) => Ok(()),
			None => Err("pass_id already exist".into()),
		}
	}

	pub fn get(
		&self,
		pass_id: &PassID,
	) -> Option<&Box<dyn Pass>> {
		self.passes.get(pass_id)
	}

	pub fn resize(
		&mut self,
		swapchain: &Swapchain,
		gbuffers: &GBuffers
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		self.passes.iter_mut().map(|(_, pass)| {
			pass.resize(swapchain, gbuffers)
		}).collect()
	}
}

pub enum RenderTarget {
	RenderPass {
		render_pass: vk::RenderPass,
		framebuffers: Vec<vk::Framebuffer>,
		subpass_count: u32,
		extent: vk::Extent2D,
		clear_values: Vec<vk::ClearValue>,
	},
	DynamicRendering,
}

pub enum AttachmentType {
	Image,
	Depth
}

impl RenderTarget {
	pub fn new(
		device: &Device,
		swapchain: &Swapchain,
		attachments: &[(AttachmentType, vk::Format, &[vk::ImageView], vk::AttachmentLoadOp, vk::AttachmentStoreOp)],
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		match &device.render_feature {
			RenderFeature::DynamicRendering(_) => Ok(RenderTarget::DynamicRendering),
			RenderFeature::RenderPass => {
				let frame_count = swapchain.frame_count as usize;
				debug_assert!(attachments.iter().all(|(_, _, views, _, _)| views.len() == frame_count));

				let descs: Vec<vk::AttachmentDescription> = attachments
					.iter()
					.map(|(ty, format, _, load_op, store_op)| {
						let final_layout = match ty {
							AttachmentType::Image => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
							AttachmentType::Depth => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
						};
						// LOAD means "preserve what's already there" - initial_layout must
						// match what it's being preserved as, not UNDEFINED (which tells
						// the driver it's free to discard the existing content).
						let initial_layout = if *load_op == vk::AttachmentLoadOp::LOAD {
							final_layout
						} else {
							vk::ImageLayout::UNDEFINED
						};
						vk::AttachmentDescription::default()
							.format(*format)
							.samples(vk::SampleCountFlags::TYPE_1)
							.load_op(*load_op)
							.store_op(*store_op)
							.initial_layout(initial_layout)
							.final_layout(final_layout)
					})
					.collect();

				let color_refs: Vec<vk::AttachmentReference> = attachments
					.iter()
					.enumerate()
					.filter(|(_, (ty, ..))| matches!(ty, AttachmentType::Image))
					.map(|(i, _)| {
						vk::AttachmentReference::default()
							.attachment(i as u32)
							.layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
					})
					.collect();

				let depth_ref: Option<vk::AttachmentReference> = attachments
					.iter()
					.enumerate()
					.find(|(_, (ty, ..))| matches!(ty, AttachmentType::Depth))
					.map(|(i, _)| {
						vk::AttachmentReference::default()
							.attachment(i as u32)
							.layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
					});

				let mut subpass = vk::SubpassDescription::default()
					.pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
					.color_attachments(&color_refs);
				if let Some(d) = &depth_ref {
					subpass = subpass.depth_stencil_attachment(d);
				}
				let subpasses = [subpass];

				let render_pass = unsafe {
					device.create_render_pass(
						&vk::RenderPassCreateInfo::default()
							.attachments(&descs)
							.subpasses(&subpasses),
						None,
					)?
				};

				let framebuffers = (0..frame_count)
					.map(|i| {
						let views: Vec<vk::ImageView> =
							attachments.iter().map(|(_, _, vs, _, _)| vs[i]).collect();
						unsafe {
							device.create_framebuffer(
								&vk::FramebufferCreateInfo::default()
									.render_pass(render_pass)
									.attachments(&views)
									.width(swapchain.extent.width)
									.height(swapchain.extent.height)
									.layers(1),
								None,
							)
						}
					})
					.collect::<Result<Vec<_>, _>>()?;

				let clear_values = Self::clear_values_for(attachments);

				Ok(RenderTarget::RenderPass { render_pass, framebuffers, subpass_count: 1, extent: swapchain.extent, clear_values })
			}
		}
	}

	/// One subpass per attachment, each attachment a depth target - lets N depth
	/// layers (e.g. shadow map array layers) render from one render pass instance
	/// via `vkCmdNextSubpass`, instead of N separate render-pass instances.
	/// `views_per_subpass[i]` is subpass i's attachment, one view per frame.
	pub fn new_multi_subpass(
		device: &Device,
		swapchain: &Swapchain,
		extent: vk::Extent2D,
		format: vk::Format,
		load_op: vk::AttachmentLoadOp,
		store_op: vk::AttachmentStoreOp,
		views_per_subpass: &[Vec<vk::ImageView>],
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		match &device.render_feature {
			RenderFeature::DynamicRendering(_) => Ok(RenderTarget::DynamicRendering),
			RenderFeature::RenderPass => {
				let frame_count = swapchain.frame_count as usize;
				debug_assert!(views_per_subpass.iter().all(|views| views.len() == frame_count));

				let subpass_count = views_per_subpass.len() as u32;

				let descs: Vec<vk::AttachmentDescription> = (0..subpass_count)
					.map(|_| {
						vk::AttachmentDescription::default()
							.format(format)
							.samples(vk::SampleCountFlags::TYPE_1)
							.load_op(load_op)
							.store_op(store_op)
							.initial_layout(vk::ImageLayout::UNDEFINED)
							.final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
					})
					.collect();

				let depth_refs: Vec<vk::AttachmentReference> = (0..subpass_count)
					.map(|i| {
						vk::AttachmentReference::default()
							.attachment(i)
							.layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
					})
					.collect();

				let subpasses: Vec<vk::SubpassDescription> = depth_refs
					.iter()
					.map(|d| {
						vk::SubpassDescription::default()
							.pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
							.depth_stencil_attachment(d)
					})
					.collect();

				let render_pass = unsafe {
					device.create_render_pass(
						&vk::RenderPassCreateInfo::default()
							.attachments(&descs)
							.subpasses(&subpasses),
						None,
					)?
				};

				let framebuffers = (0..frame_count)
					.map(|i| {
						let views: Vec<vk::ImageView> =
							views_per_subpass.iter().map(|vs| vs[i]).collect();
						unsafe {
							device.create_framebuffer(
								&vk::FramebufferCreateInfo::default()
									.render_pass(render_pass)
									.attachments(&views)
									.width(extent.width)
									.height(extent.height)
									.layers(1),
								None,
							)
						}
					})
					.collect::<Result<Vec<_>, _>>()?;

				let clear_values = vec![
					vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } };
					subpass_count as usize
				];

				Ok(RenderTarget::RenderPass { render_pass, framebuffers, subpass_count, extent, clear_values })
			}
		}
	}

	fn clear_values_for(
		attachments: &[(AttachmentType, vk::Format, &[vk::ImageView], vk::AttachmentLoadOp, vk::AttachmentStoreOp)],
	) -> Vec<vk::ClearValue> {
		attachments
			.iter()
			.map(|(ty, ..)| match ty {
				AttachmentType::Image => vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } },
				AttachmentType::Depth => vk::ClearValue {
					depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 },
				},
			})
			.collect()
	}

	pub fn resize(
		&mut self,
		device: &Device,
		swapchain: &Swapchain,
		attachments: &[(AttachmentType, vk::Format, &[vk::ImageView], vk::AttachmentLoadOp, vk::AttachmentStoreOp)],
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		match self {
			RenderTarget::RenderPass { render_pass, framebuffers, extent, clear_values, .. } => {
				let frame_count = swapchain.frame_count as usize;
				for &fb in framebuffers.iter() {
					unsafe { device.destroy_framebuffer(fb, None) };
				}
				framebuffers.clear();
				for i in 0..frame_count {
					let views: Vec<vk::ImageView> =
						attachments.iter().map(|(_, _, vs, _, _)| vs[i]).collect();
					let fb = unsafe {
						device.create_framebuffer(
							&vk::FramebufferCreateInfo::default()
								.render_pass(*render_pass)
								.attachments(&views)
								.width(swapchain.extent.width)
								.height(swapchain.extent.height)
								.layers(1),
							None,
						)
					}?;
					framebuffers.push(fb);
				}
				*extent = swapchain.extent;
				*clear_values = Self::clear_values_for(attachments);
				Ok(())
			},
			RenderTarget::DynamicRendering => {
				Ok(())
			},
		}
	}
}

#[derive(Eq, Hash, PartialEq)]
pub enum PassID {
	Back,
	Geometry,
	Shadow,
	//Lighting
}

#[derive(Borrow, BorrowInfo)]
pub struct RecordView<'v> {
	pub frame_sync: UniqueView<'v, frame_sync::FrameSync>,
	pub swapchain: UniqueView<'v, swapchain::Swapchain>,
	pub gbuffers: UniqueView<'v, gbuffers::GBuffers>,
	pub mesh_assets: UniqueView<'v, mesh::MeshAssetManager>,
	pub mesh_handles: View<'v, mesh::MeshHandle>,
	pub material_manager: UniqueView<'v, material::MaterialManager>,
	pub material_handles: View<'v, material::MaterialHandle>,
	pub transforms: View<'v, components::Transform>,
	pub frame_uniforms: UniqueView<'v, FrameUniforms>,
	pub imgui_draws: UniqueView<'v, EventQueue<Box<dyn UiDrawable>>>
}

pub trait Pass: Send + Sync {
	fn buffers(&self, frame_sync: &FrameSync) -> Vec<vk::CommandBuffer>;
	fn render_target(&self) -> &RenderTarget;
	fn record(&self, record_view: &RecordView);
	fn resize(&mut self, swapchain: &Swapchain, gbuffers: &GBuffers) -> Result<(), Box<dyn Error + Send + Sync>>;
}

