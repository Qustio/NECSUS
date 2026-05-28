use std::{error::Error, sync::Arc};
use itertools::Itertools;

use shipyard::*;
use vulkano::{descriptor_set::{DescriptorBufferInfo, DescriptorImageInfo, DescriptorSet, DescriptorSetsCollection, WriteDescriptorSet, layout::{DescriptorSetLayout, DescriptorType}, pool::{DescriptorPool, DescriptorPoolAlloc, DescriptorPoolCreateInfo, DescriptorSetAllocateInfo}}, image::ImageLayout};
use crate::modules::*;

pub struct Material {
    //desc_pool: Arc<DescriptorPool>,
    pub descriptor_sets: Vec<Arc<DescriptorSet>>,
}

impl Material {
	pub fn new(
		vulkan_context: &vulkan_context::VulkanContext,
		frames_count: u32,
		num_textures: usize,
		desc_set_layout: &Arc<DescriptorSetLayout>
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		tracing::debug!("Material::new");
        // Create descriptor pool for textures_num specified + 1 for uniform buffer
        let desc_pool_size = (
            DescriptorType::UniformBuffer,
            frames_count
        );

        let mut pool_sizes = [0..num_textures]
            .iter()
            .map(|_| {
                (
                    DescriptorType::CombinedImageSampler,
                    frames_count
                )
            })
            .collect_vec();

        pool_sizes.push(desc_pool_size);

        let desc_pool = Arc::new(DescriptorPool::new(
            &vulkan_context.device,
            &DescriptorPoolCreateInfo{
                max_sets: frames_count,
                pool_sizes: &pool_sizes,
                ..Default::default()
            }
        )?);

        let layouts = (0..frames_count).map(|_|{
            DescriptorSetAllocateInfo::new(desc_set_layout)
        }).collect::<Vec<_>>();
        // let descriptor_sets = unsafe {
        //     desc_pool.allocate_descriptor_sets(&layouts)?.collect::<Vec<_>>()
        // };

		// Alloc descriptor sets for each frame
        //let layouts = vec![DescriptorSetAllocateInfo::new(desc_set_layout); vulkan_context.frames_count as _];
        let descriptor_sets = (0..frames_count)
            .map(|_| {
                DescriptorSet::new(
                    &vulkan_context.descriptor_set_allocator, 
                    desc_set_layout,
                    &[],
                    &[]).unwrap()
            })
            .collect::<Vec<_>>();
        

		Ok(Self {
            //desc_pool,
            descriptor_sets,
        })
	}

    pub fn init_descriptors(
        &mut self,
        vulkan_context: &vulkan_context::VulkanContext,
		frames_count: u32,
        pipeline: &pipeline::Pipeline,
        uniform: &mesh::GPUUniformBuffer,
        textures: &[&texture::Texture]
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing::debug!("Material::init_descriptors");
		
		tracing::debug!("pipe bindings: {:?}", pipeline.descriptor_set_layout.bindings());
		tracing::debug!("self bindings: {:?}", self.descriptor_sets[0].layout().bindings());
		
        for i in 0..frames_count as usize {
            let ww = DescriptorBufferInfo{
                buffer: Some(&uniform.uniform_buffers[i].buffer()),
                offset: 0,
                range: None
            };
            let write_desc_set = WriteDescriptorSet::buffer(
                0,
                &ww);

            

            let mut descriptor_writes = vec![write_desc_set];

			let i_s_infos = textures
                .iter()
                .map(|t| {
                    DescriptorImageInfo{
						image_layout: ImageLayout::ShaderReadOnlyOptimal,
						image_view: Some(&t.image_view),
						sampler: Some(&t.sampler)
					}
                })
                .collect::<Vec<_>>();

            if !textures.is_empty() {
				tracing::info!("binding 1");
				tracing::debug!("{:?}", i_s_infos);
                descriptor_writes.push(
					WriteDescriptorSet::image_array(
						1,
						0,
						&i_s_infos
					)
				);
            }
			tracing::debug!("DescriptorSet::new {}", i);
            self.descriptor_sets[i] = DescriptorSet::new(
                &vulkan_context.descriptor_set_allocator,
                &pipeline.descriptor_set_layout,
                &descriptor_writes,
                &[]
            )?;
			tracing::debug!("Done");
        }

        Ok(())
    }
}