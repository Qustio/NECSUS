use crate::prelude::*;
use ash::khr::copy_commands2;
use ash::prelude::VkResult;
use ash::*;
use shipyard::*;
use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::{error::Error, ffi::CStr};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[derive(Unique)]
pub struct VulkanContext {
	#[cfg(feature = "gpu_label")]
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
		window: &Arc<winit::window::Window>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let display_handle = window.display_handle()?.as_raw();
		let window_handle = window.window_handle()?.as_raw();
		let instance = Instance::new(name, version, display_handle)?;
		#[cfg(feature = "gpu_label")]
		let debug_msg = DebugMsg::new(instance.clone())?;
		let surface = Surface::new(
			instance.clone(),
			display_handle,
			window_handle,
			window.clone(),
		)?;
		let device = Device::new(instance.clone(), &surface)?;
		let allocator = Allocator::new(instance.clone(), device.clone())?;

		Ok(Self {
			instance,
			#[cfg(feature = "gpu_label")]
			debug_msg,
			surface,
			device,
			allocator,
		})
	}

	pub(super) fn recreate_surface(
		&mut self,
		window: &Arc<winit::window::Window>,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		self.device.wait()?;
		let display_handle = window.display_handle()?.as_raw();
		let window_handle = window.window_handle()?.as_raw();
		self.surface = Surface::new(
			self.instance.clone(),
			display_handle,
			window_handle,
			window.clone(),
		)?;
		Ok(())
	}
}

#[derive(DDebug, DDeref)]
pub struct Instance {
	#[debug(skip)]
	#[deref]
	pub(super) instance: ash::Instance,
	#[debug(skip)]
	entry: ash::Entry,
}

impl Instance {
	#[tracing::instrument(name = "Instance::new", skip_all)]
	fn new(
		name: &str,
		version: u32,
		display_handle: winit::raw_window_handle::RawDisplayHandle,
	) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
		let entry = unsafe { ash::Entry::load()? };
		let surface_extensions = ash_window::enumerate_required_extensions(display_handle)?;
		let instance = unsafe {
			let app_name = CString::new(name)?;

			let app_info = vk::ApplicationInfo::default()
				.application_name(&app_name)
				.application_version(version)
				.api_version(vk::API_VERSION_1_1);

			let mut extensions = surface_extensions.to_vec();
			#[cfg(feature = "gpu_label")]
			extensions.push(ext::debug_utils::NAME.as_ptr());

			let mut layers = Vec::<*const std::os::raw::c_char>::new();
			#[cfg(feature = "gpu_label")] {
				extensions.push(ext::validation_features::NAME.as_ptr());
				layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr() as *const std::os::raw::c_char);
			}

			#[cfg(feature = "gpu_label")]
			let mut debug_messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
				.message_severity(
					vk::DebugUtilsMessageSeverityFlagsEXT::INFO
						| vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
						| vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
				)
				.message_type(
					vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
						| vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
						| vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
				)
				.pfn_user_callback(Some(DebugMsg::vulkan_debug_callback));

			let instance_info = vk::InstanceCreateInfo::default()
				.application_info(&app_info)
				.enabled_extension_names(&extensions)
				.enabled_layer_names(&layers);

			#[cfg(feature = "gpu_label")]
			let instance_info = instance_info.push_next(&mut debug_messenger_info);

			entry.create_instance(&instance_info, None)?
		};
		Ok(Arc::new(Self { instance, entry }))
	}
}

impl Drop for Instance {
	fn drop(&mut self) {
		unsafe {
			self.instance.destroy_instance(None);
		}
	}
}

#[cfg(feature = "gpu_label")]
pub struct DebugMsg {
	messager: vk::DebugUtilsMessengerEXT,
	dbg_instance: ext::debug_utils::Instance,
	_instance: Arc<Instance>,
}

#[cfg(feature = "gpu_label")]
impl DebugMsg {
	fn new(instance: Arc<Instance>) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
		let dbg_instance = ext::debug_utils::Instance::new(&instance.entry, &instance);
		let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
			.message_severity(
				vk::DebugUtilsMessageSeverityFlagsEXT::INFO
					| vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
					| vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
			)
			.message_type(
				vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
					| vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
					| vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
			)
			.pfn_user_callback(Some(DebugMsg::vulkan_debug_callback));
		let messager = unsafe { dbg_instance.create_debug_utils_messenger(&info, None)? };
		Ok(Arc::new(Self {
			messager,
			dbg_instance,
			_instance: instance,
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
			Ok(message_utf8) => match severity {
				Flag::VERBOSE => tracing::trace!("{msg_type:?} - {message_utf8}"),
				Flag::INFO => tracing::info!("{msg_type:?} - {message_utf8}"),
				Flag::WARNING => tracing::warn!("{msg_type:?} - {message_utf8}"),
				_ => tracing::error!("{msg_type:?} - {message_utf8}"),
			},
			Err(e) => tracing::error!("vulkan_debug_callback UTF8 parsing error - {e}"),
		}

		vk::FALSE
	}
}

#[cfg(feature = "gpu_label")]
impl Drop for DebugMsg {
	fn drop(&mut self) {
		unsafe {
			self.dbg_instance
				.destroy_debug_utils_messenger(self.messager, None);
		}
	}
}

#[derive(DDebug, DDeref)]
pub struct Surface {
	#[debug(skip)]
	#[deref]
	surface_instance: khr::surface::Instance,
	pub(super) surface: vk::SurfaceKHR,
	_window: Arc<winit::window::Window>,
}

impl Surface {
	fn new(
		instance: Arc<Instance>,
		display_handle: winit::raw_window_handle::RawDisplayHandle,
		window_handle: winit::raw_window_handle::RawWindowHandle,
		window: Arc<winit::window::Window>,
	) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
		let surface = unsafe {
			ash_window::create_surface(
				&instance.entry,
				&instance,
				display_handle,
				window_handle,
				None,
			)?
		};
		let surface_instance = khr::surface::Instance::new(&instance.entry, &instance);
		Ok(Arc::new(Self {
			surface_instance,
			surface,
			_window: window,
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

#[derive(DDebug, DDeref)]
pub struct Device {
	#[debug(skip)]
	#[deref]
	pub(super) device: ash::Device,
	pub(super) physical_device: vk::PhysicalDevice,
	pub(super) graphics_queue_index: u32,
	pub graphics_queue: Mutex<vk::Queue>,
	pub sync_feature: SyncFeature,
	pub render_feature: RenderFeature,
	pub copy_feature: CopyFeature,
	#[cfg(feature = "gpu_label")]
	#[debug(skip)]
	debug_utils: ext::debug_utils::Device,
	_instance: Arc<Instance>,
}

#[derive(DDebug)]
pub enum RenderFeature {
	RenderPass,
	DynamicRendering(#[debug(skip)] khr::dynamic_rendering::Device),
}

impl RenderFeature {
	/// Panics if the device is in classic RenderPass mode. Callers must only reach
	/// this after confirming (via their own RenderTarget variant) dynamic rendering
	/// is the active backend for this pass.
	pub fn dynamic_rendering(&self) -> &khr::dynamic_rendering::Device {
		match self {
			RenderFeature::DynamicRendering(loader) => loader,
			RenderFeature::RenderPass => {
				panic!("dynamic rendering requested on a RenderPass-only device")
			}
		}
	}
}

fn vulkan_array_to_slice<'a, T>(
	pointer: *const T,
	count: u32
) -> &'a [T] {
	if count > 0 {
		unsafe {
			std::slice::from_raw_parts(pointer, count as usize)
		}
	} else {
		&[]
	}
}

#[derive(DDebug)]
pub enum SyncFeature {
	Synchronization1,
	Synchronization2(#[debug(skip)] khr::synchronization2::Device),
}

impl SyncFeature {
	pub unsafe fn cmd_pipeline_barrier2(
		&self,
		device: &ash::Device,
		command_buffer: vk::CommandBuffer,
		dependency_info: &vk::DependencyInfo,
	) {
		match self {
			SyncFeature::Synchronization2(sync2_device) => unsafe {
				sync2_device.cmd_pipeline_barrier2(command_buffer, dependency_info);
			},
			SyncFeature::Synchronization1 => unsafe {
				let memory_barriers = vulkan_array_to_slice(
					dependency_info.p_memory_barriers,
					dependency_info.memory_barrier_count
				);
				let buffer_barriers = vulkan_array_to_slice(
					dependency_info.p_buffer_memory_barriers,
					dependency_info.buffer_memory_barrier_count,
				);
				let image_barriers = vulkan_array_to_slice(
					dependency_info.p_image_memory_barriers,
					dependency_info.image_memory_barrier_count,
				);

				// sync2's flag bits are numerically compatible with the classic 32-bit
				// flags for every stage/access that existed pre-1.3 - a raw truncating
				// cast is the standard downgrade path (only loses bits for stages/access
				// that sync2 introduced, which a device lacking sync2 can't have anyway).
				let downgrade_stage = |f: vk::PipelineStageFlags2| {
					vk::PipelineStageFlags::from_raw(f.as_raw() as u32)
				};
				let downgrade_access =
					|f: vk::AccessFlags2| vk::AccessFlags::from_raw(f.as_raw() as u32);

				// classic vkCmdPipelineBarrier takes one src/dst stage mask for the whole
				// call, not per-barrier - OR every barrier's stages together.
				let mut src_stage_mask = vk::PipelineStageFlags::empty();
				let mut dst_stage_mask = vk::PipelineStageFlags::empty();

				let classic_memory_barriers: Vec<vk::MemoryBarrier> = memory_barriers
					.iter()
					.map(|b| {
						src_stage_mask |= downgrade_stage(b.src_stage_mask);
						dst_stage_mask |= downgrade_stage(b.dst_stage_mask);
						vk::MemoryBarrier::default()
							.src_access_mask(downgrade_access(b.src_access_mask))
							.dst_access_mask(downgrade_access(b.dst_access_mask))
					})
					.collect();

				let classic_buffer_barriers: Vec<vk::BufferMemoryBarrier> = buffer_barriers
					.iter()
					.map(|b| {
						src_stage_mask |= downgrade_stage(b.src_stage_mask);
						dst_stage_mask |= downgrade_stage(b.dst_stage_mask);
						vk::BufferMemoryBarrier::default()
							.src_access_mask(downgrade_access(b.src_access_mask))
							.dst_access_mask(downgrade_access(b.dst_access_mask))
							.src_queue_family_index(b.src_queue_family_index)
							.dst_queue_family_index(b.dst_queue_family_index)
							.buffer(b.buffer)
							.offset(b.offset)
							.size(b.size)
					})
					.collect();

				let classic_image_barriers: Vec<vk::ImageMemoryBarrier> = image_barriers
					.iter()
					.map(|b| {
						src_stage_mask |= downgrade_stage(b.src_stage_mask);
						dst_stage_mask |= downgrade_stage(b.dst_stage_mask);
						vk::ImageMemoryBarrier::default()
							.src_access_mask(downgrade_access(b.src_access_mask))
							.dst_access_mask(downgrade_access(b.dst_access_mask))
							.old_layout(b.old_layout)
							.new_layout(b.new_layout)
							.src_queue_family_index(b.src_queue_family_index)
							.dst_queue_family_index(b.dst_queue_family_index)
							.image(b.image)
							.subresource_range(b.subresource_range)
					})
					.collect();

				// an empty mask on a barrier-only call is invalid - fall back to the
				// widest legal bracket if nothing set it.
				if src_stage_mask.is_empty() {
					src_stage_mask = vk::PipelineStageFlags::TOP_OF_PIPE;
				}
				if dst_stage_mask.is_empty() {
					dst_stage_mask = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
				}

				device.cmd_pipeline_barrier(
					command_buffer,
					src_stage_mask,
					dst_stage_mask,
					dependency_info.dependency_flags,
					&classic_memory_barriers,
					&classic_buffer_barriers,
					&classic_image_barriers,
				);
			},
		}
	}

	pub unsafe fn queue_submit2(
		&self,
		device: &ash::Device,
		queue: vk::Queue,
        submits: &[vk::SubmitInfo2KHR],
        fence: vk::Fence,
	) -> VkResult<()> {
		match self {
			SyncFeature::Synchronization1 => unsafe {
				let downgrade_stage =
					|f: vk::PipelineStageFlags2| vk::PipelineStageFlags::from_raw(f.as_raw() as u32);

				struct ClassicSubmit {
					wait_semaphores: Vec<vk::Semaphore>,
					wait_stage_masks: Vec<vk::PipelineStageFlags>,
					command_buffers: Vec<vk::CommandBuffer>,
					signal_semaphores: Vec<vk::Semaphore>,
				}

				let owned: Vec<ClassicSubmit> = submits
					.iter()
					.map(|s| {
						let wait_infos = std::slice::from_raw_parts(
							s.p_wait_semaphore_infos,
							s.wait_semaphore_info_count as usize,
						);
						let cmd_infos = std::slice::from_raw_parts(
							s.p_command_buffer_infos,
							s.command_buffer_info_count as usize,
						);
						let signal_infos = std::slice::from_raw_parts(
							s.p_signal_semaphore_infos,
							s.signal_semaphore_info_count as usize,
						);

						ClassicSubmit {
							wait_semaphores: wait_infos.iter().map(|w| w.semaphore).collect(),
							wait_stage_masks: wait_infos
								.iter()
								.map(|w| downgrade_stage(w.stage_mask))
								.collect(),
							command_buffers: cmd_infos.iter().map(|c| c.command_buffer).collect(),
							signal_semaphores: signal_infos.iter().map(|sig| sig.semaphore).collect(),
						}
					})
					.collect();

				let classic_submits: Vec<vk::SubmitInfo> = owned
					.iter()
					.map(|s| {
						vk::SubmitInfo::default()
							.wait_semaphores(&s.wait_semaphores)
							.wait_dst_stage_mask(&s.wait_stage_masks)
							.command_buffers(&s.command_buffers)
							.signal_semaphores(&s.signal_semaphores)
					})
					.collect();

				device.queue_submit(queue, &classic_submits, fence)
			},
			SyncFeature::Synchronization2(device) => unsafe {
				device.queue_submit2(queue, submits, fence)
			},
		}
	}
}


#[derive(DDebug)]
pub enum CopyFeature {
	Default,
	CopyCommands2,
}

impl CopyFeature {
	pub unsafe fn cmd_blit_image2(
		&self,
		device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        blit_image_info: &vk::BlitImageInfo2,
	) {
		match self {
			CopyFeature::Default => unsafe {
				let regions = std::slice::from_raw_parts(
					blit_image_info.p_regions,
					blit_image_info.region_count as usize
				);
				let regions = regions.iter().map(|r| {
					vk::ImageBlit {
						src_subresource: r.src_subresource,
						src_offsets: r.src_offsets,
						dst_subresource: r.dst_subresource,
						dst_offsets: r.dst_offsets,
					}
				}).collect::<Vec<_>>();
				device.cmd_blit_image(
					command_buffer,
					blit_image_info.src_image,
					blit_image_info.src_image_layout,
					blit_image_info.dst_image,
					blit_image_info.dst_image_layout,
					&regions,
					blit_image_info.filter
				);
			},
			CopyFeature::CopyCommands2 => unsafe {
				device.cmd_blit_image2(command_buffer, blit_image_info)
			},
		}
	}
}

impl Device {
	#[tracing::instrument(name = "Device::new", skip_all)]
	fn new(
		instance: Arc<Instance>,
		surface: &Surface,
	) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
		tracing::trace!("enumerating devices...");
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
		tracing::trace!("enumerating devices done");

		let dynamic_rendering_present;
		let synchronization2_present;
		let copy2_present;
		let device = unsafe {
			let mut extensions = vec![khr::swapchain::NAME];

			let available_extensions: std::collections::HashSet<std::ffi::CString> = instance
				.enumerate_device_extension_properties(physical_device)?
				.iter()
				.map(|p| p.extension_name_as_c_str().unwrap().to_owned())
				.collect();

			// VK_KHR_swapchain
			if !available_extensions.contains(khr::swapchain::NAME) {
				todo!("VK_KHR_swapchain not available? headless?")
			}

			// VK_KHR_dynamic_rendering
			let mut dynamic_rendering_features =
				vk::PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true);
			dynamic_rendering_present = available_extensions.contains(khr::dynamic_rendering::NAME)
				&& std::env::var("VK13_DISABLE").is_err();
			tracing::info!(
				"{:?} present: {}",
				khr::dynamic_rendering::NAME,
				dynamic_rendering_present
			);
			if dynamic_rendering_present {
				extensions.push(khr::dynamic_rendering::NAME);
			}

			// VK_KHR_synchronization2
			let mut synchronization2_features =
				vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);
			synchronization2_present = available_extensions.contains(khr::synchronization2::NAME);
			//synchronization2_present = false;
			tracing::info!(
				"{:?} present: {}",
				khr::synchronization2::NAME,
				dynamic_rendering_present
			);
			if synchronization2_present {
				extensions.push(khr::synchronization2::NAME);
			}

			// VK_KHR_copy_commands2
			copy2_present = available_extensions.contains(khr::copy_commands2::NAME);
			//copy2_present = false;
			tracing::info!(
				"{:?} present: {}",
				khr::copy_commands2::NAME,
				dynamic_rendering_present
			);
			if copy2_present {
				extensions.push(khr::copy_commands2::NAME);
			}

			for extension in extensions.iter() {
				tracing::info!(
					"Extension {:?} present: {}",
					extension,
					available_extensions.contains(*extension)
				);
			}

			let enabled_extension_names = extensions
				.into_iter()
				.map(|e| e.as_ptr())
				.collect::<Vec<_>>();
			let enabled_features = vk::PhysicalDeviceFeatures::default();
			let queue_create_infos = [vk::DeviceQueueCreateInfo::default()
				.queue_family_index(queue_family_index)
				.queue_priorities(&[1.0_f32])];

			let mut info = vk::DeviceCreateInfo::default()
				.enabled_extension_names(&enabled_extension_names)
				.enabled_features(&enabled_features)
				.queue_create_infos(&queue_create_infos);

			if dynamic_rendering_present {
				info = info.push_next(&mut dynamic_rendering_features);
			}
			if synchronization2_present {
				info = info.push_next(&mut synchronization2_features);
			}

			instance.create_device(physical_device, &info, None)?
		};

		let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

		#[cfg(feature = "gpu_label")]
		let debug_utils = ext::debug_utils::Device::new(&instance, &device);

		let render_feature = if dynamic_rendering_present {
			RenderFeature::DynamicRendering(khr::dynamic_rendering::Device::new(&instance, &device))
		} else {
			RenderFeature::RenderPass
		};
		let sync_feature = if synchronization2_present {
			SyncFeature::Synchronization2(khr::synchronization2::Device::new(&instance, &device))
		} else {
			SyncFeature::Synchronization1
		};
		let copy_feature = if copy2_present {
			CopyFeature::CopyCommands2
		}  else {
			CopyFeature::Default
		};

		Ok(Arc::new(Self {
			device,
			physical_device,
			graphics_queue_index: queue_family_index,
			graphics_queue: Mutex::new(queue),
			render_feature,
			sync_feature,
			copy_feature,
			_instance: instance,
			#[cfg(feature = "gpu_label")]
			debug_utils,
		}))
	}

	#[cfg(feature = "gpu_label")]
	pub fn cmd_begin_label(&self, cmd: vk::CommandBuffer, name: &str, color: [f32; 4]) {
		let name = CString::new(name).unwrap_or_default();
		unsafe {
			self.debug_utils.cmd_begin_debug_utils_label(
				cmd,
				&vk::DebugUtilsLabelEXT::default()
					.label_name(&name)
					.color(color),
			);
		}
	}

	#[cfg(feature = "gpu_label")]
	pub fn cmd_end_label(&self, cmd: vk::CommandBuffer) {
		unsafe {
			self.debug_utils.cmd_end_debug_utils_label(cmd);
		}
	}

	#[cfg(feature = "gpu_label")]
	pub fn cmd_insert_label(&self, cmd: vk::CommandBuffer, name: &str, color: [f32; 4]) {
		let name = CString::new(name).unwrap_or_default();
		unsafe {
			self.debug_utils.cmd_insert_debug_utils_label(
				cmd,
				&vk::DebugUtilsLabelEXT::default()
					.label_name(&name)
					.color(color),
			);
		}
	}

	#[cfg(feature = "gpu_label")]
	pub fn set_label<T: vk::Handle>(&self, object_handle: T, name: &str) {
		let name = CString::new(name).unwrap_or_default();
		unsafe {
			let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
				.object_handle(object_handle)
				.object_name(&name);
			self.debug_utils
				.set_debug_utils_object_name(&name_info)
				.unwrap();
		}
	}

	#[cfg(not(feature = "gpu_label"))]
	pub fn cmd_begin_label(&self, _cmd: vk::CommandBuffer, _name: &str, _color: [f32; 4]) {}

	#[cfg(not(feature = "gpu_label"))]
	pub fn cmd_end_label(&self, _cmd: vk::CommandBuffer) {}

	#[cfg(not(feature = "gpu_label"))]
	pub fn cmd_insert_label(&self, _cmd: vk::CommandBuffer, _name: &str, _color: [f32; 4]) {}

	#[cfg(not(feature = "gpu_label"))]
	pub fn set_label<T: vk::Handle>(&self, _object_handle: T, _name: &str) {}

	pub fn wait_queue(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
		let q = self.graphics_queue.lock().expect("couldnt lock queue");
		unsafe {
			self.queue_wait_idle(*q)?;
		}
		Ok(())
	}

	pub fn wait(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
		unsafe {
			self.device_wait_idle()?;
		}
		Ok(())
	}
}

impl Drop for Device {
	fn drop(&mut self) {
		unsafe {
			self.destroy_device(None);
		}
	}
}

#[derive(derive_more::Deref)]
pub struct Allocator {
	#[deref]
	allocator: vk_mem::Allocator,
	_device: Arc<Device>,
}

impl Allocator {
	fn new(
		instance: Arc<Instance>,
		device: Arc<Device>,
	) -> Result<Arc<Self>, Box<dyn Error + Send + Sync>> {
		let allocator = unsafe {
			vk_mem::Allocator::new(vk_mem::AllocatorCreateInfo::new(
				&instance,
				&device,
				device.physical_device,
			))?
		};
		Ok(Arc::new(Self {
			allocator,
			_device: device,
		}))
	}
}
