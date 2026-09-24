pub mod allocated_image;
pub mod buffer;
pub mod command_context;
pub mod debug_tools;
pub mod frame_sync;
pub mod gbuffers;
pub mod imgui;
pub mod light;
pub mod material;
pub mod mesh;
pub mod pass;
pub mod swapchain;
pub mod vulkan_context;

use crate::modules::core::EventQueue;
use crate::modules::renderer::imgui::UiDrawable;
use crate::modules::renderer::pass::{Pass, RecordView};
use crate::prelude::*;

use crate::{
	State,
	modules::{
		self, Module, System, components,
		core::{AppData, Time},
		renderer::{
			material::standart::{FrameUniforms, StandartMaterial},
		},
		window::Window,
	},
};
use nalgebra_glm::Vec3;
use shipyard::{
	AllStoragesViewMut, Borrow, BorrowInfo, IntoIter, Label, UniqueView, UniqueViewMut, View,
	scheduler::IntoWorkloadTrySystem,
};
use std::ops::Deref;
use std::{error::Error, sync::atomic::Ordering};
use winit::event::WindowEvent;

pub struct RendererModule;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Label)]
pub struct Render;

/// Run directly (not part of the per-frame state cycle) from `resumed()` when Android
/// hands back a new window after backgrounding - rebuilds only what that invalidates
/// (Surface, Swapchain, GBuffers), leaving Instance/Device/pipelines/imgui/meshes alone.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Label)]
pub struct ReattachSurface;

impl Module for RendererModule {
	fn build(engine: &mut crate::Engine) -> Result<(), Box<dyn std::error::Error>> {
		engine.register_event::<Box<dyn UiDrawable>>();
		let pos = engine
			.states
			.iter()
			.position(|s| s.dyn_eq(&State::PostUpdate))
			.unwrap();
		engine.states.insert(pos, Box::new(Render));

		engine.systems.push(System::new(
			Box::new(State::Startup),
			setup_renderer.into_workload_try_system()?,
		));
		engine.systems.push(
			System::new(Box::new(Render), render_start.into_workload_try_system()?)
				.label("FrameStart")
				.before("Record"),
		);
		engine.systems.push(
			System::new(
				Box::new(Render),
				render_update_frame_uniforms.into_workload_try_system()?,
			)
			.after("FrameStart")
			.before("Record"),
		);
		engine.systems.push(
			System::new(
				Box::new(Render),
				render_record_shadows.into_workload_try_system()?,
			)
			.label("Record"),
		);
		engine.systems.push(
			System::new(
				Box::new(Render),
				render_record_main.into_workload_try_system()?,
			)
			.label("Record"),
		);
		engine.systems.push(
			System::new(
				Box::new(Render),
				render_record_back.into_workload_try_system()?,
			)
			.label("Record"),
		);
		engine.systems.push(
			System::new(
				Box::new(Render),
				render_record_imgui.into_workload_try_system()?,
			)
			.label("Imgui record")
			.after("Record"),
		);
		engine.systems.push(
			System::new(Box::new(Render), render_submit.into_workload_try_system()?)
				.after("Record")
				.after("Imgui record"),
		);
		engine.systems.push(
			System::new(
				Box::new(State::Cleanup),
				render_wait.into_workload_try_system()?,
			)
			.label("Wait idle"),
		);
		engine.systems.push(System::new(
			Box::new(State::PreUpdate),
			imgui_handle_events.into_workload_try_system()?,
		));
		engine.systems.push(
			System::new(
				Box::new(State::PreUpdate),
				recreate_swapchain.into_workload_try_system()?,
			)
			.label("Recreate swapchain"),
		);

		engine.systems.push(System::new(
			Box::new(State::Update),
			capture_frame_ui.into_workload_try_system()?,
		));
		engine.systems.push(System::new(
			Box::new(ReattachSurface),
			reattach_surface.into_workload_try_system()?,
		));
		Ok(())
	}
}

fn imgui_handle_events(
	mut imgui_state: UniqueViewMut<imgui::ImguiState>,
	events: UniqueView<modules::core::EventQueue<WindowEvent>>,
	window: UniqueView<modules::window::Window>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();

	imgui_state.handle_events(events, window)?;
	Ok(())
}

fn render_start(
	mut frame_sync: UniqueViewMut<frame_sync::FrameSync>,
	swapchain: UniqueView<swapchain::Swapchain>,
	gbuffers: UniqueView<gbuffers::GBuffers>,
	cmd_ctx: UniqueView<command_context::CommandContext>,
	pass_manager: UniqueView<pass::PassManager>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!("render_start");
	_span.emit_color(0xFF2255);

	frame_sync.wait()?;
	swapchain.acqure(&mut frame_sync).expect("OUT OF DATE");

	cmd_ctx.begin(frame_sync.frame_id)?;
	cmd_ctx.to_optimal(&frame_sync, &swapchain, &gbuffers)?;

	// clearing image - can be one call
	// legacy mode: Main's own render pass already uses CLEAR load op, so this is a
	// redundant second render-pass instance on the same framebuffer with no barrier
	// between them - only needed for dynamic rendering, where Main's secondary LOADs.
	let is_dynamic = matches!(
		cmd_ctx.commands[frame_sync.frame_id as usize].device.render_feature,
		vulkan_context::RenderFeature::DynamicRendering(_)
	);
	if is_dynamic {
		let main_pass = pass_manager.get(&pass::PassID::Geometry).expect("couldnt get main_pass from pass_manager");
		cmd_ctx.begin_rendering(&frame_sync, &swapchain, &gbuffers, &main_pass.render_target())?;
		cmd_ctx.end_rendering(&frame_sync)?;
	}
	Ok(())
}

fn capture_frame_ui(
	mut draw_list: UniqueViewMut<EventQueue<Box<dyn UiDrawable>>>,
	#[cfg(feature = "tracy")]
	capture: UniqueView<debug_tools::FrameCapture>,
	lights: View<light::DirectionalLight>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	#[cfg(feature = "tracy")]
	let pending = capture.pending.clone();
	let positions: Vec<Vec3> = lights.iter().map(|l| l.position).collect();

	draw_list.push(Box::new(move |ui: &Ui| {
		#[cfg(feature = "tracy")]
		ui.window("Capture frame").build(|| {
			if ui.button("capture") {
				tracing::info!("captured frame");
				pending.store(true, Ordering::Relaxed);
			}
		});
		for (i, d) in positions.iter().enumerate() {
			ui.window(format!("Light {} pos", i)).build(|| {
				ui.text_colored([1.0, 0.2, 0.2, 1.0], format!("x: {}", d.x));
				ui.text_colored([0.2, 1.0, 0.2, 1.0], format!("y: {}", d.y));
				ui.text_colored([0.2, 0.2, 1.0, 1.0], format!("z: {}", d.z));
			});
		}
	}));
	Ok(())
}

#[tracing::instrument(skip_all)]
fn render_record_main(
	pass_manager: UniqueView<pass::PassManager>,
	record_view: RecordView
) -> Result<(), Box<dyn Error + Send + Sync>> {
	pass_manager.get(&pass::PassID::Geometry).map(|pass| pass.record(&record_view));
	Ok(())
}

#[tracing::instrument(skip_all)]
fn render_record_shadows(
	pass_manager: UniqueView<pass::PassManager>,
	record_view: RecordView
) -> Result<(), Box<dyn Error + Send + Sync>> {
	pass_manager.get(&pass::PassID::Shadow).map(|pass| pass.record(&record_view));
	Ok(())
}

fn render_update_frame_uniforms(
	frame_sync: UniqueView<frame_sync::FrameSync>,
	swapchain: UniqueView<swapchain::Swapchain>,
	frame_uniforms: UniqueView<FrameUniforms>,
	camera: UniqueView<components::Camera>,
	time: UniqueView<Time>,
	lights: View<light::DirectionalLight>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF2255);

	let id = frame_sync.frame_id as usize;
	frame_uniforms.update(
		id,
		&swapchain.extent,
		&camera,
		time.elapsed.as_secs_f32(),
		&lights,
	);
	Ok(())
}

fn render_record_back(
	frame_sync: UniqueView<frame_sync::FrameSync>,
	//back_pass: UniqueView<pass::back::Back>,
	swapchain: UniqueView<swapchain::Swapchain>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	//back_pass.record(&frame_sync, &swapchain)?;
	Ok(())
}


#[tracing::instrument(skip_all)]
fn render_record_imgui(
	record_view: RecordView,
	// frame_sync: UniqueView<frame_sync::FrameSync>,
	imgui_pass: UniqueView<imgui::ImguiState>,
	// swapchain: UniqueView<swapchain::Swapchain>,
	// draw_list: UniqueView<EventQueue<Box<dyn UiDrawable>>>,
	// window: UniqueView<Window>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	imgui_pass.record(&record_view)?;
	Ok(())
}

fn render_submit(
	mut frame_sync: UniqueViewMut<frame_sync::FrameSync>,
	cmd_ctx: UniqueView<command_context::CommandContext>,
	swapchain: UniqueView<swapchain::Swapchain>,
	pass_manager: UniqueView<pass::PassManager>,
	imgui_pass: UniqueView<imgui::ImguiState>,
	capture: UniqueView<debug_tools::FrameCapture>,
	gbuffers: UniqueView<gbuffers::GBuffers>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0x5566AA);

	let main_pass = pass_manager.get(&pass::PassID::Geometry).expect("couldnt get main_pass from pass_manager").deref();
	let shadow_pass = pass_manager.get(&pass::PassID::Shadow).expect("couldnt get main_pass from pass_manager").deref();
	let imgui_pass = imgui_pass.deref();
	cmd_ctx.execute_commands(&frame_sync, &swapchain, &gbuffers, shadow_pass)?;
	cmd_ctx.shadow_to_readable(&frame_sync, &swapchain, &gbuffers)?;
	cmd_ctx.execute_commands(&frame_sync, &swapchain, &gbuffers, main_pass)?;
	cmd_ctx.execute_commands(&frame_sync, &swapchain, &gbuffers, imgui_pass)?;
	cmd_ctx.swapchain_to_present(&frame_sync, &swapchain, &capture)?;
	cmd_ctx.end(&frame_sync)?;
	cmd_ctx.submit(&frame_sync, &capture)?;
	swapchain.present(&mut frame_sync)?;
	tracy_client::frame_mark();

	Ok(())
}

fn render_wait(
	vulkan_context: UniqueViewMut<vulkan_context::VulkanContext>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	vulkan_context.device.wait()?;
	Ok(())
}

fn recreate_swapchain(
	mut swapchain: UniqueViewMut<swapchain::Swapchain>,
	mut gbuffers: UniqueViewMut<gbuffers::GBuffers>,
	events: UniqueView<modules::core::EventQueue<WindowEvent>>,
	mut pass_manager: UniqueViewMut<pass::PassManager>,
	mut imgui_pass: UniqueViewMut<imgui::ImguiState>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();

	let new_size = events.events.iter().find_map(|e| match e {
		WindowEvent::Resized(size) => Some(*size),
		_ => None,
	});
	if let Some(new_size) = new_size {
		let mut new_extent = swapchain.recreate(new_size)?;
		// Android: surface capabilities can briefly lag the real post-rotation
		// buffer geometry, handing back a swapped width/height. Re-querying
		// right after settles onto the real value.
		if (new_extent.width > new_extent.height) != (new_size.width > new_size.height) {
			new_extent = swapchain.recreate(new_size)?;
		}
		gbuffers.resize(new_extent)?;

		// only rebuild framebuffers on an actual resize - this used to run every
		// frame unconditionally, leaking a framebuffer per frame per pass (see the
		// destroy fix in RenderTarget::resize) and degrading FPS until it crashed.
		pass_manager.resize(&swapchain, &gbuffers)?;
		// imgui_pass isn't in pass_manager (it's a separate Unique) - its framebuffers
		// cache the old swapchain image views, which Swapchain::recreate() just destroyed.
		imgui_pass.resize(&swapchain, &gbuffers)?;
	}

	Ok(())
}

fn reattach_surface(world: AllStoragesViewMut) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	let window = world.get_unique::<&Window>()?;
	let size = window.window.inner_size();

	{
		let mut context = world.get_unique::<&mut vulkan_context::VulkanContext>()?;
		context.recreate_surface(&window.window)?;
	}

	let new_swapchain = {
		let context = world.get_unique::<&vulkan_context::VulkanContext>()?;
		swapchain::Swapchain::new(
			context.instance.clone(),
			context.device.clone(),
			context.surface.clone(),
			size,
			None,
		)?
	};
	let new_extent = new_swapchain.extent;

	let mut imgui_state = world.get_unique::<&mut imgui::ImguiState>()?;
	imgui_state.reattach_window(window.clone())?;

	let mut gbuffers = world.get_unique::<&mut gbuffers::GBuffers>()?;
	gbuffers.resize(new_extent)?;

	// Main/Shadow/Imgui's framebuffers still point at the old swapchain's (now
	// destroyed) image views - rebuild them against the new swapchain before it
	// replaces the old one, or the next begin_render_pass dereferences freed memory.
	let mut pass_manager = world.get_unique::<&mut pass::PassManager>()?;
	pass_manager.resize(&new_swapchain, &gbuffers)?;
	imgui_state.resize(&new_swapchain, &gbuffers)?;

	world.add_unique(new_swapchain);

	Ok(())
}

fn setup_renderer(world: AllStoragesViewMut) -> Result<(), Box<dyn Error + Send + Sync>> {
	tracing::info!("setup_renderer: start");
	let _span = tracy_client::span!();
	let app_data = world.get_unique::<&AppData>()?;
	let window = world.get_unique::<&Window>()?;
	let size = window.window.inner_size();
	tracing::info!("setup_renderer: window size {:?}", size);

	// Create context
	let context =
		vulkan_context::VulkanContext::new(app_data.name, app_data.version, &window.window)?;

	// Create swapchain
	let swapchain = swapchain::Swapchain::new(
		context.instance.clone(),
		context.device.clone(),
		context.surface.clone(),
		size,
		None,
	)?;

	// Frame sync data
	let frame_sync = frame_sync::FrameSync::new(context.device.clone(), swapchain.frame_count)?;

	// Create command context
	let command_context =
		command_context::CommandContext::new(context.device.clone(), swapchain.frame_count)?;

	let gbuffers = gbuffers::GBuffers::new(
		swapchain.extent,
		context.allocator.clone(),
		context.device.clone(),
		swapchain.frame_count,
	)?;

	// Create passes
	let main_pass = pass::main::Main::new(
		context.device.clone(),
		&swapchain,
		&gbuffers
	)?;
	let shadow_pass = pass::shadow::Shadow::new(
		context.device.clone(),
		&swapchain,
		&gbuffers
	)?;
	// let back_pass = pass::back::Back::new(
	// 	context.device.clone(),
	// 	context.allocator.clone(),
	// 	swapchain.frame_count,
	// 	swapchain.format.format,
	// )?;
	// Build pass manager
	let mut pass_manager = pass::PassManager::new()?;
	pass_manager.insert(pass::PassID::Geometry, Box::new(main_pass));
	pass_manager.insert(pass::PassID::Shadow, Box::new(shadow_pass));

	#[cfg(target_os = "android")]
	let ini_path = world
		.get_unique::<&crate::AndroidAppHandle>()?
		.0
		.internal_data_path()
		.map(|p| p.join("imgui.ini"));
	#[cfg(not(target_os = "android"))]
	let ini_path: Option<std::path::PathBuf> = None;
	let imgui_pass = imgui::ImguiState::new(
		context.instance.clone(),
		context.device.clone(),
		&swapchain,
		window.window.clone(),
		ini_path,
	)?;
	let frame_capture = debug_tools::FrameCapture::new(
		context.device.clone(),
		context.allocator.clone(),
		swapchain.format.format,
	)?;

	let frame_uniforms = FrameUniforms::new(
		context.device.clone(),
		context.allocator.clone(),
		swapchain.frame_count,
	)?;

	#[cfg(target_os = "android")]
	let mesh_assets = mesh::MeshAssetManager::new(
		context.allocator.clone(),
		world.get_unique::<&crate::AndroidAppHandle>()?.0.clone(),
	)?;
	#[cfg(not(target_os = "android"))]
	let mesh_assets = mesh::MeshAssetManager::new(context.allocator.clone())?;
	let mut material_manager = material::MaterialManager::new()?;
	let mat = StandartMaterial::new(
		context.device.clone(),
		&swapchain,
		&gbuffers,
		&frame_uniforms,
		frame_sync.frame_count,
	)?;
	material_manager.register("standart", Box::new(mat), &pass_manager)?;

	let camera = components::Camera::default();

	world.add_unique(context);
	world.add_unique(swapchain);
	world.add_unique(frame_sync);
	world.add_unique(command_context);
	world.add_unique(pass_manager);
	world.add_unique(gbuffers);
	world.add_unique(imgui_pass);
	world.add_unique(frame_capture);
	world.add_unique(mesh_assets);
	world.add_unique(material_manager);
	world.add_unique(camera);
	world.add_unique(frame_uniforms);

	Ok(())
}
