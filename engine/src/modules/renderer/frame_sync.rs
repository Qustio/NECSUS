use std::{error::Error, sync::Arc, default::Default};

use super::vulkan_context::{Instance, Device, Surface};
use ash::{prelude::VkResult, *};
use itertools::Itertools;
use shipyard::Unique;

#[derive(Unique)]
pub struct FrameSync {
    image_availabe: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    fences: Vec<vk::Fence>,
    pub frame_id: u32,
    device: Arc<Device>
}

impl FrameSync {
    pub(super) fn new(
        device: Arc<Device>,
        frames_count: u32
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let 
        (image_availabe, render_finished, fences):
        (Vec<_>, Vec<_>, Vec<_>) =
        (0..frames_count).map(|_| unsafe {
            Ok::<_, vk::Result>((
                device.create_semaphore(&vk::SemaphoreCreateInfo::default(),None)?,
                device.create_semaphore(&vk::SemaphoreCreateInfo::default(),None)?,
                device.create_fence(&vk::FenceCreateInfo::default(), None)?
            ))
        })
        .collect::<VkResult<Vec<_>>>()?
        .into_iter()
        .multiunzip();
        Ok(Self{
            image_availabe,
            render_finished,
            fences,
            frame_id: 0,
            device
        })
    }
}

impl Drop for FrameSync {
    fn drop(&mut self) {
        unsafe {
            for &s in &self.image_availabe {
                self.device.destroy_semaphore(s, None);
            }
            for &s in &self.render_finished {
                self.device.destroy_semaphore(s, None);
            }
            for &f in &self.fences {
                self.device.destroy_fence(f, None);
            }
        }
    }
}