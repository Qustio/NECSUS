use std::sync::Arc;
use ash::*;
use super::vulkan_context::Device;


struct Material {
	device: Arc<Device>,
}