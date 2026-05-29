use std::ffi::{CString, c_char};
use std::ops::Deref;
use std::{error::Error, ffi::CStr};
use std::sync::{Arc, Mutex};
use shipyard::*;
use ash::{*};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

const ENABLE_VALIDATION: bool = cfg!(debug_assertions);

#[derive(Unique)]
pub struct VulkanContext {
    pub debug_msg: Arc<DebugMsg>,
    pub surface: Arc<Surface>,
    pub device: Arc<Device>,
    pub allocator: Arc<Allocator>,
    pub instance: Arc<Instance>,
}

impl VulkanContext {
    pub(super) fn new(
        name: &str,
        version: u32,
        window: &Arc<winit::window::Window>
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();
        let instance = Instance::new(
            name,
            version,
            display_handle
        )?;
        let debug_msg = DebugMsg::new(
            instance.clone(),
        )?;
        let surface = Surface::new(
            instance.clone(),
            display_handle,
            window_handle
        )?;
        let device = Device::new(
            instance.clone(),
            &surface
        )?;
        let allocator = Allocator::new(
            instance.clone(),
            device.clone()
        )?;

        Ok(Self {
            instance,
            debug_msg,
            surface,
            device,
            allocator
        })
    }
}

#[derive(derive_more::Deref)]
pub struct Instance {
    #[deref]
    instance: ash::Instance,
    entry: ash::Entry,
}

impl Instance {
    fn new(
        name: &str,
        version: u32,
        display_handle: winit::raw_window_handle::RawDisplayHandle
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let entry = unsafe { ash::Entry::load()? };
        let surface_extensions = ash_window::enumerate_required_extensions(display_handle)?;
        let instance =  unsafe {
            let app_name = CString::new(name)?;

            let app_info = vk::ApplicationInfo::default()
                .application_name(&app_name)
                .application_version(version)
                .api_version(vk::API_VERSION_1_3);

            let mut extensions = surface_extensions.to_vec();
            extensions.push(ext::debug_utils::NAME.as_ptr());
            extensions.push(ext::validation_features::NAME.as_ptr());

            let mut layers = Vec::<*const i8>::new();
            if ENABLE_VALIDATION {
                layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr());
            }
            
            let mut debug_messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::INFO |
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING |
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
                    vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                )
                .pfn_user_callback(Some(DebugMsg::vulkan_debug_callback));

            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&extensions)
                .enabled_layer_names(&layers)
                .push_next(&mut debug_messenger_info);

            entry.create_instance(&instance_info, None)?
        };
        Ok(Arc::new(Self {
            instance,
            entry
        }))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

pub struct DebugMsg {
    messager: vk::DebugUtilsMessengerEXT,
    dbg_instance: ext::debug_utils::Instance,
}

impl DebugMsg {
    fn new(
        instance: Arc<Instance>,
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>>{
        let dbg_instance = ext::debug_utils::Instance::new(&instance.entry, &instance);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::INFO |
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING |
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
                vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
            )
            .pfn_user_callback(Some(DebugMsg::vulkan_debug_callback));
        let messager = unsafe {
            dbg_instance.create_debug_utils_messenger(&info, None)?
        };
        Ok(Arc::new( Self {
            messager,
            dbg_instance,
        }))
    }

    unsafe extern "system" fn vulkan_debug_callback(
        severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        msg_type: vk::DebugUtilsMessageTypeFlagsEXT,
        data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        _user_data: *mut std::ffi::c_void,
    ) -> vk::Bool32 {
        use vk::DebugUtilsMessageSeverityFlagsEXT as Flag;

        let message = unsafe { CStr::from_ptr((*data).p_message) };
        match message.to_str() {
            Ok(message_utf8) => {
                match severity {
                    Flag::VERBOSE => tracing::trace!("{msg_type:?} - {message_utf8}"),
                    Flag::INFO => tracing::info!("{msg_type:?} - {message_utf8}"),
                    Flag::WARNING => tracing::warn!("{msg_type:?} - {message_utf8}"),
                    _ => tracing::error!("{msg_type:?} - {message_utf8}"),
                }
            },
            Err(e) => tracing::error!("vulkan_debug_callback UTF8 parsing error - {e}"),
        }
        
        vk::FALSE
    }
}

impl Drop for DebugMsg {
    fn drop(&mut self) {
        unsafe {
            self.dbg_instance.destroy_debug_utils_messenger(self.messager, None);
        }
    }
}

#[derive(derive_more::Deref)]
pub struct Surface {
    #[deref]
    surface_instance: khr::surface::Instance,
    pub(super) surface: vk::SurfaceKHR,
}

impl Surface {
    fn new(
        instance: Arc<Instance>,
        display_handle: winit::raw_window_handle::RawDisplayHandle,
        window_handle: winit::raw_window_handle::RawWindowHandle,
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let surface = unsafe {
            ash_window::create_surface(
                &instance.entry,
                &instance,
                display_handle,
                window_handle,
                None
            )?
        };
        let surface_instance = khr::surface::Instance::new(&instance.entry, &instance);
        Ok(Arc::new(Self {
            surface_instance,
            surface,
        }))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.surface_instance.destroy_surface(self.surface, None);
        }
    }
}

#[derive(derive_more::Deref)]
pub struct Device {
    #[deref]
    device: ash::Device,
    pub(super) physical_device: vk::PhysicalDevice,
    graphics_queue_index: u32,
    pub graphics_queue: Mutex<vk::Queue>,
    instance: Arc<Instance>,
}

impl Device {
    fn new(
        instance: Arc<Instance>,
        surface: &Surface,
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let (physical_device, queue_family_index) = unsafe {
            instance
            .enumerate_physical_devices()?
            .into_iter()
            .filter_map(|p| {
                // Some devices may not support the extensions or features that your application,
                // or report properties and limits that are not sufficient for your application.
                // These should be filtered out here.

                let queue_families = instance.get_physical_device_queue_family_properties(p);

                // Want one family that does graphics AND can present to our surface.
                queue_families.iter().enumerate().find_map(|(i, qf)| {
                    let index = i as u32;
                    let graphics_support = qf.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                    let surface_support = surface
                        .get_physical_device_surface_support(p, index, surface.surface)
                        .unwrap_or(false);

                    (graphics_support && surface_support).then_some((p, index))
                })
            })
            .min_by_key(|&(p, _)| {
                // We assign a lower score to device types that are likely to be faster/better.
                let props = instance.get_physical_device_properties(p);
                match props.device_type {
                    vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                    vk::PhysicalDeviceType::CPU => 3,
                    vk::PhysicalDeviceType::OTHER => 4,
                    _ => 5,
                }
            })
            .ok_or("no suitable physical device found")?
        };

        let device =  unsafe {
            let mut extensions = Vec::<*const c_char>::new();
            extensions.push(khr::swapchain::NAME.as_ptr());
            extensions.push(khr::shader_non_semantic_info::NAME.as_ptr());
            extensions.push(khr::buffer_device_address::NAME.as_ptr());
            extensions.push(ext::host_query_reset::NAME.as_ptr());

            let mut extensions13 = vk::PhysicalDeviceVulkan13Features::default()
                .dynamic_rendering(true)
                .synchronization2(true);

            instance.create_device(
                physical_device,
                &vk::DeviceCreateInfo::default()
                    .enabled_extension_names(&extensions)
                    .enabled_features(&vk::PhysicalDeviceFeatures::default())
                    .queue_create_infos(
                        &[vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(queue_family_index)
                            .queue_priorities(&[1.0_f32])]
                    )
                    .push_next(&mut extensions13),
                None
            )?
        };

        let queue = unsafe {
            device.get_device_queue(queue_family_index, 0)
        };

        Ok(Arc::new(Self {
            device,
            physical_device,
            graphics_queue_index: queue_family_index,
            graphics_queue: Mutex::new(queue),
            instance
        }))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
        }
    }
}

#[derive(derive_more::Deref)]
pub struct Allocator {
    #[deref]
    allocator: vk_mem::Allocator,
    device: Arc<Device>,
}

impl Allocator {
    fn new(
        instance: Arc<Instance>,
        device: Arc<Device>,
    ) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
        let allocator = unsafe {
            vk_mem::Allocator::new(
                vk_mem::AllocatorCreateInfo::new(&instance, &device, device.physical_device)
            )?
        };
        Ok(Arc::new(Self {
            allocator,
            device,
        }))
    }
}