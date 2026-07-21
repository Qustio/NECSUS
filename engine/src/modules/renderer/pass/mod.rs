use std::{ops::Deref, sync::Arc};

use ash::*;
use hashbrown::HashMap;
use shipyard::{Unique, UniqueView};

use crate::modules::renderer::vulkan_context::Allocator;

use super::frame_sync::FrameSync;

//pub mod back;
pub mod main;
pub mod shadow;

pub use super::*;

#[derive(Unique)]
pub struct PassManager {
	pub materials: HashMap<PassID, Box<dyn Pass>>,
	allocator: Arc<Allocator>,
}

#[derive(Eq, Hash, PartialEq)]
pub enum PassID {
	Back,
	Geometry,
	Shadow,
	//Lighting
}

pub trait Pass: Send + Sync {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer;
}

impl<T: Pass + Unique> Pass for UniqueView<'_, T> {
	fn buffer(&self, frame_sync: &FrameSync) -> vk::CommandBuffer {
		self.deref().buffer(frame_sync)
	}
}
