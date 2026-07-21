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

use crate::{
	State,
	modules::{
		self, Module, System, components,
		core::{AppData, Time},
		renderer::{
			imgui::UiDrawList,
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
use std::{error::Error, sync::atomic::Ordering};
use winit::event::WindowEvent;

pub struct RendererModule;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Label)]
pub struct Render;

impl Module for RendererModule {
	fn build(engine: &mut crate::Engine) -> Result<(), Box<dyn std::error::Error>> {
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
			.label("Record"),
		);
		engine.systems.push(
			System::new(Box::new(Render), render_submit.into_workload_try_system()?)
				.after("Record"),
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!("render_start");
	_span.emit_color(0xFF2255);

	frame_sync.wait()?;
	swapchain.acqure(&mut frame_sync).expect("OUT OF DATE");

	cmd_ctx.begin(frame_sync.frame_id)?;
	cmd_ctx.to_optimal(&frame_sync, &swapchain, &gbuffers)?;

	// clearing image - can be one call
	cmd_ctx.begin_rendering(&frame_sync, &swapchain, &gbuffers)?;
	cmd_ctx.end_rendering(&frame_sync)?;
	Ok(())
}

fn capture_frame_ui(
	mut draw_list: UniqueViewMut<UiDrawList>,
	capture: UniqueView<debug_tools::FrameCapture>,
	lights: View<light::DirectionalLight>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	let pending = capture.pending.clone();
	let positions: Vec<Vec3> = lights.iter().map(|l| l.position).collect();

	draw_list.items.push(Box::new(move |ui: &::imgui::Ui| {
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

#[derive(Borrow, BorrowInfo)]
struct MainRecordView<'v> {
	frame_sync: UniqueView<'v, frame_sync::FrameSync>,
	main_pass: UniqueView<'v, pass::main::Main>,
	swapchain: UniqueView<'v, swapchain::Swapchain>,
	gbuffers: UniqueView<'v, gbuffers::GBuffers>,
	mesh_assets: UniqueView<'v, mesh::MeshAssetManager>,
	mesh_handles: View<'v, mesh::MeshHandle>,
	material_manager: UniqueView<'v, material::MaterialManager>,
	material_handles: View<'v, material::MaterialHandle>,
	transforms: View<'v, components::Transform>,
	frame_uniforms: UniqueView<'v, FrameUniforms>,
}

fn render_record_main(view: MainRecordView) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	view.main_pass.record(
		&view.frame_sync,
		&view.swapchain,
		&view.gbuffers,
		&view.mesh_assets,
		&view.mesh_handles,
		&view.material_manager,
		&view.material_handles,
		&view.transforms,
		&view.frame_uniforms,
	)?;
	Ok(())
}

fn render_record_shadows(
	frame_sync: UniqueView<frame_sync::FrameSync>,
	shadow_pass: UniqueView<pass::shadow::Shadow>,
	swapchain: UniqueView<swapchain::Swapchain>,
	gbuffers: UniqueView<gbuffers::GBuffers>,
	mesh_assets: UniqueView<mesh::MeshAssetManager>,
	mesh_handles: View<mesh::MeshHandle>,
	material_manager: UniqueView<material::MaterialManager>,
	material_handles: View<material::MaterialHandle>,
	transforms: View<components::Transform>,
	frame_uniforms: UniqueView<FrameUniforms>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	shadow_pass.record(
		&frame_sync,
		&swapchain,
		&gbuffers,
		&mesh_assets,
		&mesh_handles,
		&material_manager,
		&material_handles,
		&transforms,
		&frame_uniforms,
	)?;
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

fn render_record_imgui(
	frame_sync: UniqueView<frame_sync::FrameSync>,
	imgui_pass: UniqueView<imgui::ImguiState>,
	swapchain: UniqueView<swapchain::Swapchain>,
	mut draw_list: UniqueViewMut<imgui::UiDrawList>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	imgui_pass.record(&frame_sync, &swapchain, &mut draw_list)?;
	Ok(())
}

fn render_submit(
	mut frame_sync: UniqueViewMut<frame_sync::FrameSync>,
	cmd_ctx: UniqueView<command_context::CommandContext>,
	swapchain: UniqueView<swapchain::Swapchain>,
	main_pass: UniqueView<pass::main::Main>,
	shadow_pass: UniqueView<pass::shadow::Shadow>,
	//back_pass: UniqueView<pass::back::Back>,
	imgui_pass: UniqueView<imgui::ImguiState>,
	capture: UniqueView<debug_tools::FrameCapture>,
	gbuffers: UniqueView<gbuffers::GBuffers>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0x5566AA);

	cmd_ctx.execute_commands(&frame_sync, &[&shadow_pass]);
	cmd_ctx.shadow_to_readable(&frame_sync, &swapchain, &gbuffers)?;
	cmd_ctx.execute_commands(&frame_sync, &[&main_pass, &imgui_pass]);
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();

	let new_size = events.events.iter().find_map(|e| match e {
		WindowEvent::Resized(size) => Some(*size),
		_ => None,
	});
	if let Some(new_size) = new_size {
		let new_extent = swapchain.recreate(new_size)?;
		gbuffers.resize(new_extent)?;
	}

	Ok(())
}

fn setup_renderer(world: AllStoragesViewMut) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	let app_data = world.get_unique::<&AppData>()?;
	let window = world.get_unique::<&Window>()?;
	let size = window.window.inner_size();

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

	// Create passes
	let main_pass = pass::main::Main::new(context.device.clone(), swapchain.frame_count)?;
	let shadow_pass = pass::shadow::Shadow::new(context.device.clone(), swapchain.frame_count)?;
	// let back_pass = pass::back::Back::new(
	// 	context.device.clone(),
	// 	context.allocator.clone(),
	// 	swapchain.frame_count,
	// 	swapchain.format.format,
	// )?;
	let gbuffers = gbuffers::GBuffers::new(
		swapchain.extent,
		context.allocator.clone(),
		context.device.clone(),
		swapchain.frame_count,
	)?;
	let imgui_pass = imgui::ImguiState::new(
		context.instance.clone(),
		context.device.clone(),
		swapchain.frame_count,
		swapchain.format.format,
		&window.window,
	)?;

	let draw_list = imgui::UiDrawList::default();
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

	let mesh_assets = mesh::MeshAssetManager::new(context.allocator.clone())?;
	let mut material_manager = material::MaterialManager::new()?;
	let mat = StandartMaterial::new(
		context.device.clone(),
		&swapchain,
		&gbuffers,
		&frame_uniforms,
		frame_sync.frame_count,
	)?;
	material_manager.register("standart", Box::new(mat))?;

	let camera = components::Camera::default();

	world.add_unique(context);
	world.add_unique(swapchain);
	world.add_unique(frame_sync);
	world.add_unique(command_context);
	world.add_unique(main_pass);
	world.add_unique(shadow_pass);
	//world.add_unique(back_pass);
	world.add_unique(gbuffers);
	world.add_unique(imgui_pass);
	world.add_unique(draw_list);
	world.add_unique(frame_capture);
	world.add_unique(mesh_assets);
	world.add_unique(material_manager);
	world.add_unique(camera);
	world.add_unique(frame_uniforms);

	Ok(())
}
