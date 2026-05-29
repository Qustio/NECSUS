
use super::vulkan_context::{Instance, Device, Surface};
use ash::*;
use std::{error::Error, sync::Arc};
use shipyard::{AllStoragesViewMut, Label, Unique, scheduler::IntoWorkloadTrySystem};
use crate::{State, modules::{Module, System, core::AppData, window::Window}};

#[derive(derive_more::Deref)]
pub struct CommandPool {
    #[deref]
    command_pool:   vk::CommandPool,
    device: Arc<Device>,
}

impl CommandPool {
    pub(super) fn new(
        device: Arc<Device>,
        queue_family_index: u32,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let command_pool = unsafe { 
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None
            )?
        };
        Ok(Self {
            command_pool,
            device
        })
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

pub struct FrameCommands {
    pub buffer: vk::CommandBuffer,
    pub command_pool:   Arc<CommandPool>,
}

impl FrameCommands {
    pub(super) fn new(
        command_pool: Arc<CommandPool>
    )-> Result<Self, Box<dyn Error + Send + Sync>> {
        let buffer = unsafe {
            command_pool.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(1)
                .command_pool(**command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
            )?[0]
        };
        Ok(Self{
            buffer,
            command_pool
        })
    }
}

#[derive(Unique)]
pub struct CommandContext {
    pub frames: Vec<FrameCommands>,
}