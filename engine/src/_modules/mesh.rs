use std::{
    error::Error,
    mem::{ManuallyDrop, offset_of},
    sync::{Arc, Mutex},
};
use nalgebra::allocator;
use vulkano::{buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer}, command_buffer::{AutoCommandBufferBuilder, CommandBuffer, CommandBufferUsage, CopyBufferInfo, PrimaryCommandBufferAbstract}, memory::allocator::{AllocationCreateInfo, DeviceLayout, MemoryAllocatePreference, MemoryTypeFilter}, pipeline::graphics::vertex_input, sync::GpuFuture};
use shipyard::*;
use nalgebra_glm::*;

use crate::modules::{self, *};

#[repr(C)]
#[derive(Debug, BufferContents, vertex_input::Vertex, Clone)]
pub(crate) struct Vertex {
    #[format(R32G32B32A32_SFLOAT)]
    pub(crate) pos: nalgebra_glm::Vec4,
    #[format(R32G32B32_SFLOAT)]
    pub(crate) normal: nalgebra_glm::Vec3,
    #[format(R32G32B32A32_SFLOAT)]
    pub(crate) color: nalgebra_glm::Vec4,
    #[format(R32G32_SFLOAT)]
    pub(crate) ui: nalgebra_glm::Vec2,
}


#[derive(Debug)]
pub(super) struct GPUMeshBuffers {
    pub(super) vertex_buffer: Subbuffer<[Vertex]>,
    pub(super) index_buffer: Subbuffer<[u32]>,
    pub(super) index_count: u32,
}

unsafe impl Send for GPUMeshBuffers {}
unsafe impl Sync for GPUMeshBuffers {}

impl GPUMeshBuffers {
    pub(super) fn new(
        vulkan_context: &vulkan_context::VulkanContext,
        verts: &[Vertex],
        inds: &[u32],
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        //let vertex_buffer_size = size_of_val(verts) as _;
        //let index_buffer_size = size_of_val(inds) as _;
       
        // Create index buffer
        let index_buffer = Buffer::new_slice::<u32>(
            &vulkan_context.allocator,
            &BufferCreateInfo{
                usage: BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST,
                ..Default::default()
            },
            &AllocationCreateInfo{
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
                ..Default::default()
            },
            inds.len() as u64
        )?;
        // Create vertex buffer
        let vertex_buffer = Buffer::new_slice::<Vertex>(
            &vulkan_context.allocator,
            &BufferCreateInfo{
                usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
                ..Default::default()
            },
            &AllocationCreateInfo{
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
                ..Default::default()
            },
            verts.len() as u64
        )?;

        // Create staging buffer for sending data
        let vs_buffer = Buffer::from_iter(
            &vulkan_context.allocator,
            &BufferCreateInfo{
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            &AllocationCreateInfo{
                memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_RANDOM_ACCESS,
                ..Default::default()
            },
            verts.iter().cloned()
        )?;
        let is_buffer = Buffer::from_iter(
            &vulkan_context.allocator,
            &BufferCreateInfo{
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            &AllocationCreateInfo{
                memory_type_filter: MemoryTypeFilter::PREFER_HOST | MemoryTypeFilter::HOST_RANDOM_ACCESS,
                ..Default::default()
            },
            inds.iter().cloned()
        )?;

        tracing::debug!("start copying");
        let mut buffer = AutoCommandBufferBuilder::primary(
            vulkan_context.command_buffer_allocator.clone(),
            vulkan_context.graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit
        )?;
        buffer.copy_buffer(CopyBufferInfo::new(is_buffer, index_buffer.clone()))?;
        buffer.copy_buffer(CopyBufferInfo::new(vs_buffer, vertex_buffer.clone()))?;
        let commands = buffer.build()?;
        commands.execute(vulkan_context.graphics_queue.clone())?.then_signal_fence_and_flush()?.wait(None);

        Ok(Self {
            vertex_buffer,
            index_buffer,
            index_count: inds.len() as _,
        })
    }
}

impl Drop for GPUMeshBuffers {
    fn drop(&mut self) {
        tracing::debug!("GPUMeshBuffers::Drop");
    }
}

#[repr(C)]
#[derive(Debug, Default, BufferContents)]
pub(crate) struct UniformBuffer {
    pub(super) viewproj: [[f32; 4]; 4],
    pub(super) view: [[f32; 4]; 4],
    pub(super) proj: [[f32; 4]; 4],
    pub(super) res: [f32; 3],
    pub(super) time: f32,
}

#[derive(Default, Debug, Unique)]
pub(crate) struct GPUUniformBuffer {
    pub(crate) uniform_buffers: Vec<Arc<Subbuffer<UniformBuffer>>>,
}

impl GPUUniformBuffer {
    pub(crate) fn new(
        vulkan_context: &modules::vulkan_context::VulkanContext,
		frames_count: u32,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        tracing::debug!("GPUUniformBuffer::New");

        let uniform_buffers = (0..frames_count)
            .map(|_| {
                Arc::new(Buffer::new_sized(
                    &vulkan_context.allocator,
                    &BufferCreateInfo {
                        usage: BufferUsage::UNIFORM_BUFFER,
                        ..Default::default()
                    },
                    &AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    }
                ).unwrap())
            })
            .collect::<Vec<_>>();

        Ok(Self {
            uniform_buffers,
        })
    }

    pub(super) fn update(
        &mut self,
        extent: [u32; 2],
        current_frame: u32,
		camera: &components::Camera,
        time: f32,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let view = camera.view_matrix();
		let mut proj = Mat4::new_perspective(
			extent[0] as f32 / extent[1] as f32,
			90.0f32.to_radians(),
			0.1,
			1000.0
		);
        let rev_z_matrix = Mat4::new(
            1.0, 0.0, 0.0, 0.0,
            0.0, -1.0, 0.0, 0.0,
            0.0, 0.0, -1.0, 0.0,
            0.0, 0.0, 1.0, 1.0
        );
        proj*=rev_z_matrix;
		let viewpoj = proj * view;

        let mut data = self.uniform_buffers[current_frame as usize].write()?;
        //let data = unsafe { &mut *self.uniform_mapped[current_frame as usize] };
        data.time = time;
        data.proj = proj.into();
        data.view = view.into();
        data.viewproj = viewpoj.into();
		
        data.res = [extent[0] as f32, extent[1] as f32, 0.0];

        Ok(())
    }
}
