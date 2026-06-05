use super::vulkan_context::Device;
use ash::*;
use std::sync::Arc;

struct Material {
	device: Arc<Device>,
}
