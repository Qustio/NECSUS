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
