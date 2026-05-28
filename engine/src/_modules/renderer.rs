//use buffer::{AllocatedBuffer, AllocatedImage, AllocatedImageTexture};
use bytemuck::{bytes_of, cast, cast_ref, cast_slice};
//use gltf::json::{extensions::texture, validation::Validate};
use imgui_rs_vulkan_renderer::vulkan;
use mesh::{GPUMeshBuffers, Vertex};
use nalgebra::allocator;
use shipyard::{AllStoragesViewMut, EntitiesViewMut, IntoIter, Unique, UniqueView, UniqueViewMut, View, ViewMut, Workload, World, scheduler::IntoWorkloadTrySystem};
use vk_mem::Alloc;
use vulkano::{Validated, VulkanError, VulkanObject, command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, RenderingAttachmentInfo, RenderingInfo, pool::CommandPool}, format::ClearValue, image::{ImageLayout, ImageUsage}, instance::debug::DebugUtilsLabel, pipeline::{PipelineBindPoint, graphics::viewport::{Scissor, Viewport}}, query::{QueryPool, QueryPoolCreateInfo, QueryResultFlags, QueryType}, render_pass::{AttachmentLoadOp, AttachmentStoreOp}, swapchain::{SwapchainAcquireFuture, SwapchainPresentInfo, acquire_next_image}, sync::{self, GpuFuture, PipelineStage, now}};
use crate::modules::{self, components::Camera, mesh::{GPUUniformBuffer, UniformBuffer}, pipeline::Pipeline, swapchain::Swapchain, *};
use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fmt::Debug,
    ops::DerefMut,
    sync::{Arc, Mutex},
    time::Instant,
};
use tracing::instrument;
use tracy_client::{Client, GpuContext};
use winit::{dpi::PhysicalSize, event::WindowEvent};

use crate::modules::vulkan_context::VulkanContext;

pub struct RendererModule;

#[derive(Default, Unique)]
pub struct MeshStorage {
    meshes: HashMap<String, GPUMeshBuffers>
}

#[derive(Component)]
pub struct MeshHandle(String);

#[derive(Default, Unique)]
pub struct MaterialStorage {
    materials: HashMap<String, material::Material>
}

#[derive(Unique)]
pub struct Timer(Instant);

#[derive(Component)]
pub struct MaterialHandle(String);

#[derive(Default, Unique)]
pub struct PipelineStorage {
    pipelines: HashMap<String, pipeline::Pipeline>
}

#[derive(Component)]
pub struct PipelineHandle(String);

impl Module for RendererModule {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn Error>> {
		engine.register_event::<ImguiCallback>();
        engine.systems.push(System::new(State::Startup, setup_renderer.into_workload_try_system()?));
        engine.systems.push(System::new(State::Cleanup, destroy.into_workload_try_system()?));
		//engine.systems.push(System::new(State::PreUpdate, ppp.into_workload_system()?));
		//engine.systems.push(System::new(State::Update, debug_imgui.into_workload_system()?));
		//engine.systems.push(System::new(State::Update, debug_cam.into_workload_system()?));
		engine.systems.push(System::new(State::PostUpdate, resize.into_workload_try_system()?));
		engine.systems.push(System::new(State::PostUpdate, render_frame.into_workload_try_system()?));
        Ok(())
    }
}

// fn debug_cam(
// 	imgui: NonSendSync<UniqueView<ImguiRenderable>>,
// 	mut camera: UniqueViewMut<modules::components::Camera>,
// ) -> Result<(), Box<dyn Error + Send + Sync>> {
// 	tracing::info!("debug_imgui");
// 	tracing::info!(target: "imgui", "thread id: {:?}", std::thread::current().id());
// 	if let Some(ui) = imgui.ui {
// 		tracing::info!(target: "imgui", "its gonnaexplode!!!");
// 		tracing::info!(target: "imgui", "{ui:?}");
// 		let ui = unsafe { &mut *ui };
// 		tracing::info!(target: "imgui", "boom");
// 		ui
// 			.window("camera")
// 			.build(|| {
// 				ui.text("123");
// 				ui.input_float("x", &mut camera.postition.x);
// 				ui.text(format!("{}", camera.postition.x));
// 			});
// 	}
// 	Ok(())
// }

// fn debug_imgui(
// 	mut imgui_callback: UniqueViewMut<core::EventQueue<ImguiCallback>>
// ) -> Result<(), Box<dyn Error + Send + Sync>> {
// 	tracing::info!("debug_cam");
// 	imgui_callback.push(
// 		Box::new(|ui| {
// 			ui.show_demo_window(&mut true);
// 			Ok(())
// 		})
// 	);
// 	tracing::debug!("i added ui event where the fuck is it?");
// 	Ok(())
// }


// fn ppp(
// 	world: AllStoragesViewMut,
// ) -> Result<(), Box<dyn Error + Send + Sync>> {
// 	tracing::info!(target: "imgui","ppp");
	
// 	let window_events = world.get_unique::<&core::EventQueue<winit::event::WindowEvent>>()?;
// 	if !window_events.events.contains(&winit::event::WindowEvent::RedrawRequested) {
// 		return Ok(());
// 	}
// 	let imgui_callbacks = world.get_unique::<&mut core::EventQueue<ImguiCallback>>()?;
// 	if world.get_unique::<&modules::core::Time>().is_err() {
// 		tracing::error!("NO TIME!!");
// 	}
// 	let time = world.get_unique::<&modules::core::Time>()?;
// 	let w = world.get_unique::<&modules::window::Window>().unwrap();
// 	let mut imgui = world.get_unique::<NonSendSync<&mut ImguiRenderable>>().unwrap();
// 	imgui.prepare_frame(time, w, window_events, imgui_callbacks);
// 	Ok(())
// }

pub type ImguiCallback = Box<dyn FnMut(&mut imgui::Ui) -> Result<(), Box<dyn Error + Send + Sync>> + Send + Sync>;

// #[derive(Unique)]
// pub struct ImguiRenderable {
//     //device: Device,
//     pub imgui_renderer: imgui_rs_vulkan_renderer::Renderer,
//     pub imgui_context: imgui::Context,
//     pub imgui_platform: imgui_winit_support::WinitPlatform,
//     pub ui: Option<*mut imgui::Ui>,
// }

// impl ImguiRenderable {
//     pub fn new(
//         vulkan_context: &VulkanContext,
//         allocator: &Arc<vk_mem::Allocator>,
//         swapchain: &renderer::Swapchain,
//         window: &winit::window::Window,
//         command_pool: vk::CommandPool,
//     ) -> Result<Self, Box<dyn Error + Send + Sync>> {
//         let device = vulkan_context.device.clone();

//         let mut imgui_context = imgui::Context::create();

//         let mut imgui_platform = imgui_winit_support::WinitPlatform::new(&mut imgui_context);
//         imgui_platform.attach_window(
//             imgui_context.io_mut(),
//             &window,
//             imgui_winit_support::HiDpiMode::Rounded,
//         );

//         let imgui_renderer = imgui_rs_vulkan_renderer::Renderer::with_default_allocator(
//             &vulkan_context.instance.handle(),
//             vulkan_context.physical_device,
//             device.clone(),
//             vulkan_context.graphics_queue,
//             command_pool,
//             swapchain.render_pass,
//             &mut imgui_context,
//             Some(imgui_rs_vulkan_renderer::Options {
//                 in_flight_frames: swapchain.frames_count as usize,
//                 ..Default::default()
//             }),
//         )
//         .unwrap();

//         Ok(Self {
//             //device,
//             imgui_renderer,
//             imgui_context,
//             imgui_platform,
//             ui: None
//         })
//     }

//     pub fn prepare_frame(
//         &mut self,
// 		time: UniqueView<modules::core::Time>,
// 		window: UniqueView<modules::window::Window>,
//         window_events: UniqueView<modules::core::EventQueue<winit::event::WindowEvent>>,
// 		mut ui_callbacks: UniqueViewMut<core::EventQueue<ImguiCallback>>
//     ) {
//         let io = self.imgui_context.io_mut();
// 		io.update_delta_time(time.elapsed);
// 		self.imgui_platform.prepare_frame(io, &window.window).unwrap();
// 		tracing::info!(target: "imgui", "TIME: {:?}", time.elapsed);
//         for window_event in window_events.events.iter() {
//             self.imgui_platform.handle_window_event(io, &window.window, window_event);
//         }
//         let ui = self.imgui_context.new_frame();
// 		for ui_callback in ui_callbacks.events.iter_mut() {
// 			tracing::error!("ui_callback");
// 			ui_callback(ui);
// 		}
// 		tracing::info!(target: "imgui", "thread id: {:?}", std::thread::current().id());
// 		tracing::info!(target: "imgui", "prepare_frame adress:{:?}", ui as *mut _);
//         self.ui = Some(ui as *mut _);
//     }

//     pub fn d(
//         &mut self,
//         command_buffer: vk::CommandBuffer,
//     ) {
//         let draw_data = self.imgui_context.render();
//         self.imgui_renderer.cmd_draw(command_buffer, draw_data).unwrap();
//     }
// }


fn render_frame(
	world: AllStoragesViewMut,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let window_events = world.get_unique::<&core::EventQueue<winit::event::WindowEvent>>()?;
	if !window_events.events.contains(&winit::event::WindowEvent::RedrawRequested) {
		return Ok(());
	}
	let context = world.get_unique::<&mut VulkanContext>().unwrap();
	//let imgui_callbacks = world.get_unique::<&mut core::EventQueue<ImguiCallback>>()?;
	let w = world.get_unique::<&modules::window::Window>().unwrap();
	let mut renderer = world.get_unique::<&mut Renderer>().unwrap();
	renderer.previous_frame_end.as_mut().unwrap().cleanup_finished();
	//let mut imgui = world.get_unique::<NonSendSync<&mut ImguiRenderable>>().unwrap();
	//imgui.prepare_frame(w, window_events, imgui_callbacks);
	let mut uniforms = world.get_unique::<&mut GPUUniformBuffer>().unwrap();
	let camera = world.get_unique::<&modules::components::Camera>()?;
	let mut swapchain = world.remove_unique::<Swapchain>().unwrap();
	let timer = world.get_unique::<&Timer>().unwrap();
	let mut frame_data = world.remove_unique::<FrameData>().unwrap();

	let _span = tracy_client::span!("draw_frame");

    // Update FrameData
	let last_frame_secs = frame_data.frame_time.elapsed().as_secs_f32() * 1000.0;
	frame_data.last_frame_secs = last_frame_secs;
	frame_data.frame_time = Instant::now();
	let len = frame_data.frame_times.len();
	let window = frame_data.window;
	if len == window {
		frame_data.frame_times.pop_back();
	}
	if len > window {
		frame_data.frame_times.resize(window, last_frame_secs);
	}
	frame_data.frame_times.push_front(last_frame_secs);
	frame_data.avg_frame_time = frame_data.frame_times.iter().sum::<f32>() / len as f32;

	// Acquere next frame
	let (image_index, suboptimal, acquire_future) = match acquire_next_image(swapchain.swapchain.clone(), None).map_err(Validated::unwrap) {
		Ok(r) => r,
		Err(VulkanError::OutOfDate) => {
			tracing::info!("VulkanError::OutOfDate");
			panic!("!!!");
		}
		Err(e) => {
			tracing::error!("{e}");
			panic!("!!!");
		}
	};

    // Update Uniforms
	uniforms.update(
		swapchain.extent,
		image_index,
		&camera,
		timer.0.elapsed().as_secs_f32()
	)?;

	// if self.size.width == 0 && self.size.height == 0 {
	// 	unsafe { self.vulkan_context.device.device_wait_idle()? }
	// 	return Ok(());
	// }


    // Main
	let image_index= world.run(|
		entities: EntitiesViewMut,
		vm_mesh: ViewMut<MeshHandle>,
		vm_mat: ViewMut<MaterialHandle>,
		vm_pipeline: ViewMut<PipelineHandle>,
		vm_trans: View<modules::components::Transform>,
		mesh_storage: UniqueView<MeshStorage>,
		material_storage: UniqueView<MaterialStorage>,
		pipeline_storage: UniqueView<PipelineStorage>,
	| -> Result<u32, Box<dyn Error + Send + Sync>> {
		renderer
			.record_command_buffer(
                &context,
				image_index,
				acquire_future,
				&swapchain,
				entities,
				vm_mesh,
				vm_mat,
				vm_pipeline,
				vm_trans,
				mesh_storage,
				material_storage,
				pipeline_storage,
				//imgui.deref_mut()
			)
	})?;

	renderer.frame = image_index;

	// Remove things i cant mutate for now and add them later
	// Fix this ASAP!
	world.add_unique(swapchain);
	world.add_unique(frame_data);

	if let Some(client) = Client::running() {
		client.frame_mark();
	}

	Ok(())
}

fn resize(
	world: AllStoragesViewMut,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let window_events = world.get_unique::<&core::EventQueue<winit::event::WindowEvent>>()?;
	if let Some(&size) = window_events.events.iter().find_map(|x| 
		if let winit::event::WindowEvent::Resized(size) = x {
            Some(size)
        } else {
            None
        }){
			let mut swapchain = world.get_unique::<&mut Swapchain>()?;
			let vulkan_context = world.get_unique::<&vulkan_context::VulkanContext>()?;
			swapchain.recreate(
				&vulkan_context,
				size,
			)?;
	}
	Ok(())
}

fn setup_renderer(
    world: AllStoragesViewMut,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let window = world.get_unique::<&window::Window>()?;
    let vulkan_context = VulkanContext::new(&window.window)?;
	tracing::info!("Context created");
	let camera = world.get_unique::<&modules::components::Camera>()?;
    let swapchain = Swapchain::new(&vulkan_context, window.window.inner_size()).unwrap();
	tracing::info!("Swapchain created");
    let renderer = Renderer::new(&vulkan_context, swapchain.frames_count)?;
	tracing::info!("Renderer created");
    let mut uniforms = mesh::GPUUniformBuffer::new(
        &vulkan_context,
		swapchain.frames_count
    )?;
	world.add_unique(Timer(Instant::now()));
	uniforms.update(
		swapchain.extent,
		renderer.frame,
		&camera,
		1.0
	)?;
	// let imgui = gui::ImguiRenderable::new(
	// 	&vulkan_context,
	// 	&swapchain,
	// 	&window,
	// )?;
	//world.add_unique_non_send_sync(imgui);

	// Init storages
	world.add_unique(MeshStorage::default());
	world.add_unique(MaterialStorage::default());
	world.add_unique(PipelineStorage::default());

	let mesh_storage = &mut world.get_unique::<&mut MeshStorage>()?;
	let mat_storage = &mut world.get_unique::<&mut MaterialStorage>()?;
	let pipeline_storage = &mut world.get_unique::<&mut PipelineStorage>()?;
	// let (command_pool, command_buffers) = Renderer::create_command_pool(
	// 	&vk_ctx.device,
	// 	vk_ctx.graphics_q_index,
	// 	swapchain.frames_count,
	// )?;
	let texture = texture::Texture::new(
		&vulkan_context,
		ImageUsage::SAMPLED,
		"assets/test.png",
		Some("test.png"),
	)?;
	let background_pipeline = pipeline::Pipeline::new(
		&vulkan_context,
		"./engine/shaders/background_vert.spv",
        "./engine/shaders/background_frag.spv",
        0,
		&swapchain
	)?;
	let basic_pipeline = pipeline::Pipeline::new_with_push::<modules::components::Transform>(
		&vulkan_context,
		"./engine/shaders/vert.spv",
        "./engine/shaders/frag.spv",
		1,
        &swapchain
	)?;
	
	let mut background_material = material::Material::new(
		&vulkan_context,
		swapchain.frames_count,
		0,
		&background_pipeline.descriptor_set_layout
	)?;
	let mut basic_material = material::Material::new(
		&vulkan_context,
		swapchain.frames_count,
		1,
		&basic_pipeline.descriptor_set_layout
	)?;
	let background_mesh = mesh::GPUMeshBuffers::new(
		&vulkan_context,
		&[
			Vertex{
				color: [1.0, 1.0, 1.0, 1.0].into(),
				pos: [-1.0, -1.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [-1.0, -1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [1.0, -1.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [1.0, -1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [-1.0, 1.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [-1.0, 1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [1.0, 1.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [1.0, 1.0].into(),
			}
		],
		&[0, 1, 2, 3, 2, 1]
	)?;
	let test_mesh = mesh::GPUMeshBuffers::new(
		&vulkan_context,
		&[
			Vertex{
				color: [1.0, 1.0, 1.0, 1.0].into(),
				pos: [-1.0, 0.0, -1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [-1.0, -1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [1.0, 0.0, -1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [1.0, -1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [-1.0, 0.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [-1.0, 1.0].into(),
			},
			Vertex{
				color: [1.0, 1.0, 1.0, 0.0].into(),
				pos: [1.0, 0.0, 1.0, 1.0].into(),
				normal: [1.0, 1.0, 1.0].into(),
				ui: [1.0, 1.0].into(),
			}
		],
		&[0, 1, 2, 3, 2, 1]
	)?;
	background_material.init_descriptors(
		&vulkan_context,
		swapchain.frames_count,
		&background_pipeline,
		&uniforms,
		&[]
	)?;
	tracing::debug!("done!");
	basic_material.init_descriptors(
		&vulkan_context,
		swapchain.frames_count,
		&basic_pipeline,
		&uniforms,
		&[&texture]
	)?;
	tracing::debug!("done!");
	mesh_storage.meshes.insert(
		"back_mesh".to_string(),
		background_mesh
	);
	mesh_storage.meshes.insert(
		"test_mesh".to_string(),
		test_mesh
	);
	mat_storage.materials.insert(
		"back_material".to_string(),
		background_material
	);
	mat_storage.materials.insert(
		"basic_material".to_string(),
		basic_material
	);
	pipeline_storage.pipelines.insert(
		"back_pipeline".to_string(),
		background_pipeline
	);
	pipeline_storage.pipelines.insert(
		"basic_pipeline".to_string(),
		basic_pipeline
	);
	// Temporary unique texture - might create texture storage later
	world.add_unique(texture);
	//world.add_entity(());
	world.run(|
		mut entities: EntitiesViewMut,
		mut vm_mesh: ViewMut<MeshHandle>,
		mut vm_mat: ViewMut<MaterialHandle>,
		mut vm_pipeline: ViewMut<PipelineHandle>,
		mut vm_trans: ViewMut<modules::components::Transform>,
		| {
			entities.add_entity(
				(&mut vm_mesh, &mut vm_mat, &mut vm_pipeline), 
				(
					MeshHandle("back_mesh".to_string()),
					MaterialHandle("back_material".to_string()),
					PipelineHandle("back_pipeline".to_string())
				)
			);
			entities.add_entity(
				(&mut vm_mesh, &mut vm_mat, &mut vm_pipeline, &mut vm_trans), 
				(
					MeshHandle("test_mesh".to_string()),
					MaterialHandle("basic_material".to_string()),
					PipelineHandle("basic_pipeline".to_string()),
					modules::components::Transform::default(),
				)
			);
	});
	
    //let back = renderable::background::BackgroundMaterial::new(
    //    &vk_ctx,
    //    &swapchain,
    //    &uniforms
    //)?;
    //let gltf = renderable::gltf::GLTFMaterial::new(
    //    &vk_ctx,
    //    &swapchain,
    //    &uniforms,
    //    renderer.command_pool,
    //    
    //)?;
    //let imgui =
    //    ImguiRenderable::new(&vk_ctx, &swapchain, renderer.command_pool, &window)?;
    //world.add_unique(back);
    //world.add_unique(gltf);
    //world.add_unique(imgui);
	tracing::info!("Setup eded");
    world.add_unique(vulkan_context);
    world.add_unique(swapchain);
    world.add_unique(uniforms);
    world.add_unique(renderer);
	world.add_unique(FrameData {
		time: Instant::now(),
		frame_time: Instant::now(),
		last_frame_secs: 1.0,
		frame_times: VecDeque::with_capacity(10),
		avg_frame_time: 0.0,
		window: 10,
	});
    Ok(())
}

fn destroy(
    world: AllStoragesViewMut
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _span = tracy_client::span!("destroy");
	tracing::info!("Cleanup!");
	world.remove_unique::<MaterialStorage>()?;
	world.remove_unique::<PipelineStorage>()?;
	world.remove_unique::<MeshStorage>()?;
	world.remove_unique::<GPUUniformBuffer>()?;
    world.remove_unique::<Swapchain>()?;
	//world.remove_unique::<ImguiRenderable>()?;
    world.remove_unique::<Renderer>()?;
	world.remove_unique::<VulkanContext>()?;
    Ok(())
}

#[allow(dead_code)]
#[derive(Unique)]
pub(super) struct Renderer {
    frame: u32,
    gpu_context: GpuContext,
    gpu_span: Vec<Option<tracy_client::GpuSpan>>,
    query_pool: Arc<QueryPool>,
    previous_frame_end: Option<Box<dyn GpuFuture + 'static + Send + Sync>>,
}

impl Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .finish()
    }
}

#[derive(Debug, Unique)]
pub struct FrameData {
    pub time: Instant,
    pub frame_time: Instant,
    pub last_frame_secs: f32,
    pub frame_times: VecDeque<f32>,
    pub window: usize,
    pub avg_frame_time: f32,
}

impl Renderer {
    pub fn new(
        vulkan_context: &vulkan_context::VulkanContext,
		frames_count: u32,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let query_pool = QueryPool::new(
            &vulkan_context.device,
            &QueryPoolCreateInfo {
                query_count: frames_count * 2,
                ..QueryPoolCreateInfo::new(QueryType::Timestamp)
            }
        )?;

        let client = tracy_client::Client::start();

        //////////////////

        let mut builder = AutoCommandBufferBuilder::primary(
            vulkan_context.command_buffer_allocator.clone(),
            vulkan_context.graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;
        unsafe {
            builder.write_timestamp(query_pool.clone(), 0, PipelineStage::TopOfPipe)?;
        }
        let command_buffer = builder.build()?;
        let future = now(vulkan_context.device.clone())
            .then_execute(vulkan_context.graphics_queue.clone(), command_buffer)?
            .then_signal_fence_and_flush()?;
        future.wait(None)?;

        ///////////////////////////

        tracing::debug!("!!!pool querey get start");
        let mut timestamps = [0u64; 1];
        query_pool.get_results(0, 1, &mut timestamps, QueryResultFlags::WAIT)?;

        let gpu_context = client
            .new_gpu_context(
                Some("Vulkan context"),
                tracy_client::GpuContextType::Vulkan,
                timestamps[0] as i64,
                vulkan_context.physical_device.properties().timestamp_period,
            )
            .unwrap();

        let gpu_span = vec![None, None, None, None];
        let frame = 0;
        let previous_frame_end: Option<Box<dyn GpuFuture + Send + Sync + 'static>> = Some(Box::new(sync::now(vulkan_context.device.clone())));
        Ok(Self {
            frame,
            gpu_context,
            query_pool,
            gpu_span,
            previous_frame_end
        })
    }

    pub fn record_command_buffer(
        &mut self,
        vulkan_context: &vulkan_context::VulkanContext,
		image_index: u32,
		acquire_future: SwapchainAcquireFuture,
        swapchain: &Swapchain,
		entities: EntitiesViewMut,
		vm_mesh: ViewMut<MeshHandle>,
		vm_mat: ViewMut<MaterialHandle>,
		vm_pipeline: ViewMut<PipelineHandle>,
		vm_trans: View<modules::components::Transform>,
		mesh_storage: UniqueView<MeshStorage>,
		material_storage: UniqueView<MaterialStorage>,
		pipeline_storage: UniqueView<PipelineStorage>,
		//imgui: &mut ImguiRenderable
    ) -> Result<u32, Box<dyn Error + Send + Sync>> {
        let _span = tracy_client::span!("record_command_buffer");
        let mut timestamps = [0u64; 2];
        if self.query_pool.get_results(
            self.frame * 2,
            2,
            &mut timestamps,
            QueryResultFlags::empty(),
        )? {
            if let Some(ref mut gpu_span) = self.gpu_span[self.frame as usize] {
                gpu_span.upload_timestamp_start(timestamps[0] as i64);
                gpu_span.upload_timestamp_end(timestamps[1] as i64);
            }
            if self.gpu_span[self.frame as usize].is_some() {
                self.gpu_span[self.frame as usize] = None;
            }
        }
        let mut gpu_span = self.gpu_context.span(tracy_client::span_location!())?;
        let mut builder = AutoCommandBufferBuilder::primary(
            vulkan_context.command_buffer_allocator.clone(),
            vulkan_context.graphics_queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;
        unsafe {
            builder.reset_query_pool(self.query_pool.clone(), image_index * 2..(image_index * 2 + 2))?;
            builder.write_timestamp(self.query_pool.clone(), image_index * 2, PipelineStage::TopOfPipe)?;
        }
        builder.begin_rendering(RenderingInfo {
            color_attachments: vec![Some(RenderingAttachmentInfo{
                load_op: AttachmentLoadOp::Clear,
                store_op: AttachmentStoreOp::Store,
                clear_value: Some([0.0f32, 0.0f32, 0.0f32, 0.0f32].into()),
                ..RenderingAttachmentInfo::new(
                    swapchain.image_views[image_index as usize].clone()
                )
            })],
            depth_attachment: Some(RenderingAttachmentInfo{
                load_op: AttachmentLoadOp::Clear,
                store_op: AttachmentStoreOp::Store,
                clear_value: Some(ClearValue::DepthStencil((0.0, 0))),
                ..RenderingAttachmentInfo::new(
                    swapchain.depth_image_views[image_index as usize].clone()
                )
            }),
            ..Default::default()
        })?;
        let viewport = Viewport{
            extent: [swapchain.extent[0] as f32, swapchain.extent[1] as f32],
            min_depth: 1.0f32,
            max_depth: 0.0f32,
            offset: [0.0, 0.0]
        };
		let scissor = Scissor{
			extent: [swapchain.extent[0], swapchain.extent[1]],
			offset: [0, 0]
		};
        builder.set_viewport(0, [viewport].into_iter().collect())?;
		builder.set_scissor(0, [scissor].into_iter().collect())?;
        builder.begin_debug_utils_label(DebugUtilsLabel{
            color: [1.0, 0x66 as f32 / 1.0, 0x00 as f32 / 1.0, 0xff as f32 / 1.0],
            label_name: "Scene pass".to_string(),
            ..Default::default()
        })?;

        for (
			mesh,
			material,
			pipeline,
			trans
		) in (
			&vm_mesh,
			&vm_mat,
			&vm_pipeline,
			vm_trans.as_optional()
		).iter() {
            let mesh = &mesh_storage.meshes[&mesh.0];
            let material = &material_storage.materials[&material.0];
            let pipeline = &pipeline_storage.pipelines[&pipeline.0];
            let desc_sets = material.descriptor_sets[image_index as usize].clone();
			builder.bind_pipeline_graphics(pipeline.pipeline.clone())?;
			builder.bind_descriptor_sets(
				PipelineBindPoint::Graphics,
				pipeline.pipeline_layout.clone(),
				0,
				desc_sets,
			)?;
			if let Some(trans) = trans {
				builder.push_constants(
					pipeline.pipeline_layout.clone(),
					0,
					*trans,
				)?;
			}
			builder.bind_index_buffer(mesh.index_buffer.clone())?;
			builder.bind_vertex_buffers(
				0,
				[mesh.vertex_buffer.clone()],
			)?;
			unsafe {
				builder.draw_indexed(
					mesh.index_count as _,
					1,
					0,
					0,
					0,
				)?;
			}
        }
        
        //imgui.d(command_buffer);

        builder.end_rendering()?;
        unsafe {
            builder.end_debug_utils_label()?;
        }
        unsafe {
            builder.write_timestamp(self.query_pool.clone(), image_index * 2 + 1, PipelineStage::BottomOfPipe)?;
        }
        let command_buffer = builder.build()?;

        gpu_span.end_zone();
        self.gpu_span[image_index as usize] = Some(gpu_span);
        let future = self.previous_frame_end.take().unwrap()
            .join(acquire_future)
            .then_execute(vulkan_context.graphics_queue.clone(), command_buffer).unwrap()
            .then_swapchain_present(
                vulkan_context.graphics_queue.clone(),
                SwapchainPresentInfo::new(swapchain.swapchain.clone(), image_index)
            )
            .then_signal_fence_and_flush();
        match future.map_err(Validated::unwrap) {
            Ok(future) => {
                self.previous_frame_end = Some(future.boxed_send_sync());
            },
            Err(VulkanError::OutOfDate) => {
                self.previous_frame_end = Some(sync::now(vulkan_context.device.clone()).boxed_send_sync());
            },
            Err(e) => {
                tracing::error!("failed to flush future: {e}");
                self.previous_frame_end = Some(sync::now(vulkan_context.device.clone()).boxed_send_sync());
            }
        }
        Ok(image_index)
    }
}
