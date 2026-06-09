pub mod standart;

use hashbrown::HashMap;
use shipyard::{Component, Unique};

pub use std::{error::Error, sync::Arc};
pub use super::command_context::FrameCommand;
pub use super::vulkan_context::{Allocator, Device};
pub use super::pass::PassID;
pub use ash::*;


#[derive(Unique)]
pub struct MaterialManager {
	materials: HashMap<String, Box<dyn Materal>>,
    pipelines: HashMap<(String, PassID), Pipeline>,
}

impl MaterialManager {
    pub(super) fn new(
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(Self {
            materials: HashMap::default(),
            pipelines: HashMap::default(),
        })
    }

    pub fn register(
        &mut self,
        name: &str, material: Box<dyn Materal>
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let pipelines = material.create_pipeline()?;
        for (pass_id, pipeline) in pipelines {
            self.pipelines.insert((name.to_string(), pass_id), pipeline);
        }
        self.materials.insert(name.to_string(), material);
        Ok(())
    }
    pub fn get_material(&self, material: &str) -> Option<&Box<dyn Materal>> {
        self.materials.get(&material.to_string())
    }
    pub fn get_pipeline(&self, material: &str, pass: PassID) -> Option<&Pipeline> {
        let key = (material.to_string(), pass);
        self.pipelines.get(&key)
    }
}

#[derive(Component)]
pub struct MaterialHandle(pub String);

pub struct BindContext<'a> {
    pub transform: &'a crate::modules::components::Transform,
    pub camera: Option<&'a crate::modules::components::Camera>,
	pub light: Option<&'a crate::modules::components::DirectionalLight>,
    pub extent: &'a vk::Extent2D,
    pub frame_id: usize,
}

pub trait Materal: Send + Sync {
    fn create_pipeline(&self) -> Result<Vec<(PassID, Pipeline)>, Box<dyn Error + Send + Sync>>;
    fn bind(&self, pass_id: PassID, cmd: &FrameCommand, pipeline: &Pipeline, ctx: &BindContext);
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


pub fn build_shader(
	name: &str,
	entry_points: &[&str],
) -> Result<slang::Blob, Box<dyn Error + Send + Sync>> {
	let global_session = slang::GlobalSession::new().ok_or("Slang not found")?;
	let target_desc = slang::TargetDesc::default()
		.format(slang::CompileTarget::Spirv)
		.profile(global_session.find_profile("glsl_450"));
	let targets = [target_desc];
	let binding = slang::CompilerOptions::default()
			.matrix_layout_column(true);
	let paths = [c".".as_ptr()];
	let session_desc = slang::SessionDesc::default()
		.targets(&targets)
		.search_paths(&paths)
		.options(&binding);
	let session = global_session.create_session(&session_desc).unwrap();
	let module = session.load_module(name)?;

	// Collect into components
	let mut components = entry_points.iter().map(|&ep| {
		module.find_entry_point_by_name(ep)
			.ok_or(format!("entry point {} not found", ep)).unwrap().into()
	}).collect::<Vec<slang::ComponentType>>();
	components.insert(0, module.into());

	let program = session
		.create_composite_component_type(&components)?;
	let linked = program.link()?;
	let code = linked.target_code(0)?;

	Ok(code)
}