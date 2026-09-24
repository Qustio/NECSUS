pub mod standart;

use hashbrown::HashMap;
use shipyard::{Component, Unique};

pub use super::command_context::FrameCommand;
pub use super::pass::{PassID, PassManager};
pub use super::vulkan_context::{Allocator, Device};
pub use ash::*;
pub use std::{error::Error, sync::Arc};

#[derive(Unique)]
pub struct MaterialManager {
	materials: HashMap<String, Box<dyn Materal>>,
	// subpass index - pipeline is only valid within the exact subpass
	// single subpass (Main, Imgui) always use 0.
	pipelines: HashMap<(String, PassID, u32), Pipeline>,
}

impl MaterialManager {
	pub(super) fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
		Ok(Self {
			materials: HashMap::default(),
			pipelines: HashMap::default(),
		})
	}

	pub fn register(
		&mut self,
		name: &str,
		material: Box<dyn Materal>,
		pass_manager: &PassManager
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let pipelines = material.create_pipeline(pass_manager)?;
		for (pass_id, subpass, pipeline) in pipelines {
			self.pipelines.insert((name.to_string(), pass_id, subpass), pipeline);
		}
		self.materials.insert(name.to_string(), material);
		Ok(())
	}
	pub fn get_material(&self, material: &str) -> Option<&Box<dyn Materal>> {
		self.materials.get(&material.to_string())
	}
	pub fn get_pipeline(&self, material: &str, pass: PassID, subpass: u32) -> Option<&Pipeline> {
		let key = (material.to_string(), pass, subpass);
		self.pipelines.get(&key)
	}
}

#[derive(Component)]
pub struct MaterialHandle(pub String);

pub struct BindContext<'a> {
	pub transform: &'a crate::modules::components::Transform,
	pub frame_uniforms: &'a standart::FrameUniforms,
	/// `Some` during the shadow pass (the light's own view*proj for this draw); ignored by the geometry pass.
	pub light_view_proj: Option<nalgebra_glm::Mat4>,
	pub extent: &'a vk::Extent2D,
	pub frame_id: usize,
}

pub trait Materal: Send + Sync {
	fn create_pipeline(&self, pass_manager: &PassManager) -> Result<Vec<(PassID, u32, Pipeline)>, Box<dyn Error + Send + Sync>>;
	fn bind(
		&self,
		id: u32,
		pass_id: PassID,
		cmd: &FrameCommand,
		pipeline: &Pipeline,
		ctx: &BindContext,
	);
}

pub struct Pipeline {
	pub pipeline: vk::Pipeline,
	pub layout: vk::PipelineLayout,
	pub device: Arc<Device>,
}

impl Drop for Pipeline {
	fn drop(&mut self) {
		unsafe {
			self.device.destroy_pipeline_layout(self.layout, None);
			self.device.destroy_pipeline(self.pipeline, None);
		}
	}
}
