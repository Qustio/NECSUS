use std::{collections::HashMap, error::Error, sync::Arc};

use crate::modules::vulkan_context;

use shipyard::*;
use crate::modules::*;

#[derive(Unique)]
pub struct BackgroundMaterial {
    device: Device,
    pipelines: HashMap<RenderTarget, Pipeline>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Component)]
pub struct BM;

impl BackgroundMaterial {
    pub fn new(
        vulkan_context: &vulkan_context::VulkanContext,
        swapchain: &swapchain::Swapchain,
        uniform: &mesh::GPUUniformBuffer
    ) -> Result<Self, Box<dyn Error>> {
        let device = vulkan_context.device.clone();

        // Create pipelines
        let mut pipelines = HashMap::new();
        pipelines.insert(
            RenderTarget::MainPass,
            Pipeline::new(
                vulkan_context,
                "./uwuengine/shaders/background_vert.spv",
                "./uwuengine/shaders/background_frag.spv",
                swapchain.extent,
                swapchain.render_pass,
                &[],
            )?,
        );

        for (_, pipeline) in pipelines.iter() {
                Pipeline::init_descriptors(
                    &device,
                    &uniform.uniform_buffers,
                    &pipeline.descriptor_sets,
                    &[],
                    swapchain.frames_count,
                )?;
            }

        Ok(Self { device, pipelines })
    }
}