use std::ops::Deref;

use ash::*;
use shipyard::{Unique, UniqueView};

use super::frame_sync::FrameSync;

pub mod back;
pub mod main;

pub use super::*;

pub(crate) trait Pass {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer;
}

impl<T: Pass + Unique> Pass for UniqueView<'_, T> {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		self.deref().buffer(frame_sync)
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