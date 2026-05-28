use ash::{Device, ext, khr, vk};
use bytemuck::checked::cast_slice;
use std::{collections::HashMap, error::Error, sync::Arc};

use shipyard::*;
use crate::modules::*;

use super::{Pipeline, RenderTarget, Renderable};

pub struct GLTFMaterial {
    device: Device,
    textures: Vec<buffer::AllocatedImageTexture>,
    pipelines: HashMap<RenderTarget, Pipeline>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Component)]
pub struct GLTFM;

impl GLTFMaterial {
    pub fn new(
        vulkan_context: &vulkan_context::VulkanContext,
        swapchain: &renderer::Swapchain,
        uniform: &mesh::GPUUniformBuffer,
        command_pool: vk::CommandPool,
    ) -> Result<Self, Box<dyn Error>> {
        let device = vulkan_context.device.clone();
        let allocator = &vulkan_context.allocator;

        let texture = buffer::AllocatedImageTexture::new(
            &device,
            allocator,
            &vulkan_context.debug_device,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageUsageFlags::SAMPLED,
            vk_mem::AllocationCreateFlags::empty(),
            vulkan_context.graphics_queue,
            command_pool,
            "assets/test.png",
            Some(c"TestTexture"),
        )?;

        let textures = vec![texture];

        // Create pipelines
        let mut pipelines = HashMap::new();
        pipelines.insert(
            RenderTarget::MainPass,
            Pipeline::new_with_push_constants(
                vulkan_context,
                "./uwuengine/shaders/vert.spv",
                "./uwuengine/shaders/frag.spv",
                swapchain.extent,
                swapchain.render_pass,
                &textures,
            )?,
        );

        for (_, pipeline) in pipelines.iter() {
                Pipeline::init_descriptors(
                    &device,
                    &uniform.uniform_buffers,
                    &pipeline.descriptor_sets,
                    &textures,
                    swapchain.frames_count,
                )?;
            }

        Ok(Self {
            device,
            pipelines,
            textures,
        })
    }
}

impl Renderable for GLTFMaterial {
    fn draw(
        &mut self,
        command_buffer: vk::CommandBuffer,
        material_pass: RenderTarget,
        world: &mut World,
        frame: u32,
    ) -> Result<(), Box<dyn Error>> {
        // match material_pass {
        //     RenderTarget::MainPass => {
        //         let current_pass_pipeline = self.pipelines.get(&RenderTarget::MainPass).unwrap();
        //         let desc_sets = [current_pass_pipeline.descriptor_sets[frame as usize]];
        //         let pipeline = current_pass_pipeline.pipeline;
        //         let pipeline_layout = current_pass_pipeline.pipeline_layout;
        //         let mut meshes = world.query::<(&MeshAsset, &GLTFM, &Transform)>();
        //         unsafe {
        //             self.device.cmd_bind_pipeline(
        //                 command_buffer,
        //                 vk::PipelineBindPoint::GRAPHICS,
        //                 pipeline,
        //             );
        //             self.device.cmd_bind_descriptor_sets(
        //                 command_buffer,
        //                 vk::PipelineBindPoint::GRAPHICS,
        //                 pipeline_layout,
        //                 0,
        //                 &desc_sets,
        //                 &[],
        //             );

        //             for (mesh, _, trans) in meshes.iter(world) {
        //                 let cols: [[f32; 4]; 4] = trans.local.into();
		// 				//tracing::debug!("{:?}", cols);
        //                 let push_constants = cast_slice(&cols);
		// 				//tracing::debug!("{:?}", push_constants);
        //                 self.device.cmd_push_constants(
        //                     command_buffer,
        //                     pipeline_layout,
        //                     vk::ShaderStageFlags::VERTEX,
        //                     0,
        //                     push_constants,
        //                 );
        //                 self.device.cmd_bind_index_buffer(
        //                     command_buffer,
        //                     mesh.mesh_buffers.index_buffer.buffer,
        //                     0,
        //                     vk::IndexType::UINT32,
        //                 );
        //                 self.device.cmd_bind_vertex_buffers(
        //                     command_buffer,
        //                     0,
        //                     &[mesh.mesh_buffers.vertex_buffer.buffer],
        //                     &[0],
        //                 );
        //                 self.device.cmd_draw_indexed(
        //                     command_buffer,
        //                     mesh.mesh_buffers.index_count as _,
        //                     1,
        //                     0,
        //                     0,
        //                     0,
        //                 );
        //             }
        //         }
        //     }
        //     RenderTarget::BackgroundPass => (),
        //     RenderTarget::DebugPass => (),
        // }
        Ok(())
    }

    fn update(&mut self, world: &mut World) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
