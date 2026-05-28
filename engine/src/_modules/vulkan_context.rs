use std::error::Error;
use std::sync::Arc;
use shipyard::*;

use crate::_modules::{self, Module};


#[derive(Unique)]
pub struct VulkanContext {
    pub(crate) instance: Arc<Instance>,
    pub(crate) debug_utils_messenger: Arc<DebugUtilsMessenger>,
    pub(crate) physical_device: Arc<PhysicalDevice>,
    pub(crate) device: Arc<Device>,
    pub(crate) graphics_queue: Arc<Queue>,
    pub(crate) surface: Arc<Surface>,
    pub(crate) allocator: Arc<StandardMemoryAllocator>,
    pub(crate) command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    pub(crate) descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
}

impl VulkanContext {
    pub(crate) fn new(window: &Arc<winit::window::Window>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let library = unsafe { VulkanLibrary::new()? };
        let required_extensions = Surface::required_extensions(window).unwrap();
        let instance = Instance::new(
            &library,
            &InstanceCreateInfo {
                // Enable enumerating devices that use non-conformant Vulkan implementations.
                // (e.g. MoltenVK)
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: &InstanceExtensions {
                    ext_debug_utils: true,
					ext_validation_features: true,
                    ..required_extensions
                },             
                enabled_layers: &["VK_LAYER_KHRONOS_validation"],
				enabled_validation_features: &[ValidationFeatureEnable::DebugPrintf],
                ..Default::default()
            },
        )
        .unwrap();
        let debug_utils_messenger = Self::create_debug_messenger(&instance)?;
        let surface = Surface::from_window(&instance, window)?;
        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
			khr_shader_non_semantic_info: true,
            khr_buffer_device_address: true,
            ext_host_query_reset: true,
            ..DeviceExtensions::empty()
        };
        let (physical_device, graphics_q) =
            Self::pick_physical_device(&instance, window, &device_extensions)?;
        let (device, graphics_queue) = Self::create_logical_device(
            &physical_device,
            &device_extensions,
            graphics_q
        )?;
        let allocator = Self::create_allocator(&device)?;
        let command_buffer_allocator =
            Arc::new(StandardCommandBufferAllocator::new(&device, &Default::default()));
        let descriptor_set_allocator =
            Arc::new(StandardDescriptorSetAllocator::new(&device, &Default::default()));
        let capabilities =
            physical_device.surface_capabilities(&surface, &SurfaceInfo::default())?;
        Ok(Self {
            instance,
            debug_utils_messenger,
            physical_device,
            device,
            graphics_queue,
            surface,
            allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
        })
    }

    fn create_debug_messenger(
        instance: &Arc<Instance>,
    ) -> Result<Arc<DebugUtilsMessenger>, Box<dyn Error + Send + Sync>> {
        let debug_callback = unsafe {
            DebugUtilsMessenger::new(
                instance,
                &DebugUtilsMessengerCreateInfo {
                    message_severity: DebugUtilsMessageSeverity::ERROR
                        | DebugUtilsMessageSeverity::WARNING
                        | DebugUtilsMessageSeverity::INFO
                        | DebugUtilsMessageSeverity::VERBOSE,
                    message_type: DebugUtilsMessageType::GENERAL
                        | DebugUtilsMessageType::VALIDATION
                        | DebugUtilsMessageType::PERFORMANCE,
                    ..DebugUtilsMessengerCreateInfo::new(&DebugUtilsMessengerCallback::new(
                        |message_severity, message_type, callback_data| {
                            let ty = if message_type.intersects(DebugUtilsMessageType::GENERAL) {
                                "general"
                            } else if message_type.intersects(DebugUtilsMessageType::VALIDATION) {
                                "validation"
                            } else if message_type.intersects(DebugUtilsMessageType::PERFORMANCE) {
                                "performance"
                            } else {
                                panic!("no-impl");
                            };

                            if message_severity
                                .intersects(DebugUtilsMessageSeverity::ERROR)
                            {
                                tracing::error!(
                                    "{} {}: {}",
                                    callback_data.message_id_name.unwrap_or("unknown"),
                                    ty,
                                    callback_data.message
                                );
                            } else if message_severity.intersects(DebugUtilsMessageSeverity::WARNING) {
                                tracing::warn!(
                                    "{} {}: {}",
                                    callback_data.message_id_name.unwrap_or("unknown"),
                                    ty,
                                    callback_data.message
                                );
                            } else if message_severity.intersects(DebugUtilsMessageSeverity::INFO) {
                                tracing::info!(
                                    "{} {}: {}",
                                    callback_data.message_id_name.unwrap_or("unknown"),
                                    ty,
                                    callback_data.message
                                );
                            } else if message_severity.intersects(DebugUtilsMessageSeverity::VERBOSE) {
                                tracing::debug!(
                                    "{} {}: {}",
                                    callback_data.message_id_name.unwrap_or("unknown"),
                                    ty,
                                    callback_data.message
                                );
                            } else {
                                panic!("no-impl");
                            };
                        },
                    ))
                },
            )
        }?;
        let debug_callback = Arc::new(debug_callback);
        Ok(debug_callback)
    }

    fn pick_physical_device(
        instance: &Arc<Instance>,
        window: &Arc<winit::window::Window>,
        device_extensions: &DeviceExtensions
    ) -> Result<(Arc<PhysicalDevice>, u32), Box<dyn Error + Send + Sync>> {
        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| {
                // Some devices may not support the extensions or features that your application,
                // or report properties and limits that are not sufficient for your application.
                // These should be filtered out here.
                p.supported_extensions().contains(&device_extensions)
            })
            .filter_map(|p| {
                // For each physical device, we try to find a suitable queue family that will
                // execute our draw commands.
                //
                // Devices can provide multiple queues to run commands in parallel (for example a
                // draw queue and a compute queue), similar to CPU threads. This is
                // something you have to have to manage manually in Vulkan. Queues
                // of the same type belong to the same queue family.
                //
                // Here, we look for a single queue family that is suitable for our purposes. In a
                // real-world application, you may want to use a separate dedicated transfer queue
                // to handle data transfers in parallel with graphics operations.
                // You may also need a separate queue for compute operations, if
                // your application uses those.
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        // We select a queue family that supports graphics operations. When drawing
                        // to a window surface, as we do in this example, we also need to check
                        // that queues in this queue family are capable of presenting images to the
                        // surface.
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.presentation_support(i as u32, window).unwrap()
                    })
                    // The code here searches for the first queue family that is suitable. If none
                    // is found, `None` is returned to `filter_map`, which
                    // disqualifies this physical device.
                    .map(|i| (p, i as u32))
            })
            // All the physical devices that pass the filters above are suitable for the
            // application. However, not every device is equal, some are preferred over others.
            // Now, we assign each physical device a score, and pick the device with the lowest
            // ("best") score.
            //
            // In this example, we simply select the best-scoring device to use in the application.
            // In a real-world setting, you may want to use the best-scoring device only as a
            // "default" or "recommended" device, and let the user choose the device themself.
            .min_by_key(|(p, _)| {
                // We assign a lower score to device types that are likely to be faster/better.
                match p.properties().device_type {
                    PhysicalDeviceType::DiscreteGpu => 0,
                    PhysicalDeviceType::IntegratedGpu => 1,
                    PhysicalDeviceType::VirtualGpu => 2,
                    PhysicalDeviceType::Cpu => 3,
                    PhysicalDeviceType::Other => 4,
                    _ => 5,
                }
            })
            .expect("no suitable physical device found");
        Ok((physical_device, queue_family_index))
    }

    fn create_logical_device(
        physical_device: &Arc<PhysicalDevice>,
        device_extensions: &DeviceExtensions,
        graphics_q_index: u32,
    ) -> Result<(Arc<Device>, Arc<Queue>), Box<dyn Error + Send + Sync>> {
        let (device, mut queues) = Device::new(
            // Which physical device to connect to.
            &physical_device,
            &DeviceCreateInfo {
                // A list of optional features and extensions that our program needs to work
                // correctly. Some parts of the Vulkan specs are optional and must be enabled
                // manually at device creation. In this example the only thing we are going to need
                // is the `khr_swapchain` extension that allows us to draw to a window.
                enabled_extensions: &device_extensions,
                enabled_features: &DeviceFeatures {
					logic_op: true,
					dynamic_rendering: true,
                    sampler_anisotropy: true,
                    ..DeviceFeatures::empty()
                },

                // The list of queues that we are going to use. Here we only use one queue, from
                // the previously chosen queue family.
                queue_create_infos: &[QueueCreateInfo {
                    queue_family_index: graphics_q_index,
                    ..Default::default()
                }],

                ..Default::default()
            },
        )
        .unwrap();

        // Since we can request multiple queues, the `queues` variable is in fact an iterator. We
        // only use one queue in this example, so we just retrieve the first and only element of
        // the iterator.
        let queue = queues.next().unwrap();
        Ok((device, queue))
    }

    fn create_allocator(
        device: &Arc<Device>,
    ) -> Result<Arc<StandardMemoryAllocator>, Box<dyn Error + Send + Sync>> {
        let allocator = Arc::new(StandardMemoryAllocator::new(&device, &Default::default()));
        Ok(allocator)
    }
}