pub mod vulkan_context;
pub mod swapchain;
pub mod frame_sync;
pub mod command_context;
pub mod pass;
pub mod buffer;
pub mod mesh;
pub mod pipeline;
pub mod material;

use std::error::Error;
use shipyard::{AllStoragesViewMut, Label, UniqueView, UniqueViewMut, scheduler::IntoWorkloadTrySystem};
use winit::event::WindowEvent;
use crate::{State, modules::{self, Module, System, core::AppData, renderer::pass::Pass, window::Window}};

pub struct RendererModule;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Label)]
pub struct Render;

impl Module for RendererModule {
    fn build(engine: &mut crate::Engine) -> Result<(), Box<dyn std::error::Error>> {
        let pos = engine.states.iter().position(|s| s.dyn_eq(&State::PostUpdate)).unwrap();
        engine.states.insert(pos, Box::new(Render));

        engine.systems.push(
            System::new(Box::new(State::Startup), setup_renderer.into_workload_try_system()?)
        );
        engine.systems.push(
            System::new(Box::new(Render), render_start.into_workload_try_system()?)
            .before("Record")
        );
		engine.systems.push(
            System::new(Box::new(Render), render_record_main.into_workload_try_system()?)
            .label("Record")
        );
		engine.systems.push(
            System::new(Box::new(Render), render_record_back.into_workload_try_system()?)
            .label("Record")
        );
        engine.systems.push(
            System::new(Box::new(Render), render_submit.into_workload_try_system()?)
            .after("Record")
        );
		engine.systems.push(
            System::new(Box::new(State::Cleanup), render_wait.into_workload_try_system()?)
            .label("Wait idle")
        );
		engine.systems.push(
            System::new(Box::new(State::PreUpdate), recreate_swapchain.into_workload_try_system()?)
            .label("Recreate swapchain")
        );
        Ok(())
    }
}

fn render_start(
	mut frame_sync: UniqueViewMut<frame_sync::FrameSync>,
	swapchain: UniqueView<swapchain::Swapchain>,
	cmd_ctx: UniqueView<command_context::CommandContext>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!("render_start");
	_span.emit_color(0xFF2255);

	frame_sync.wait()?;
	swapchain.acqure(&mut frame_sync).expect("OUT OF DATE");

	cmd_ctx.begin(frame_sync.frame_id)?;
	cmd_ctx.swapchain_to_optimal(&frame_sync, &swapchain)?;

	// clearing image - can be one call
	cmd_ctx.begin_rendering(&frame_sync, &swapchain)?;
	cmd_ctx.end_rendering(&frame_sync)?;
    Ok(())
}

fn render_record_main(
	rame_sync: UniqueView<frame_sync::FrameSync>,
	main_pass: UniqueView<pass::Main>,
	swapchain: UniqueView<swapchain::Swapchain>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	main_pass.record(&rame_sync, &swapchain);
    Ok(())
}

fn render_record_back(
	rame_sync: UniqueView<frame_sync::FrameSync>,
	back_pass: UniqueView<pass::Back>,
	swapchain: UniqueView<swapchain::Swapchain>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	let _span = tracy_client::span!();
	_span.emit_color(0xFF6600);

	back_pass.record(&rame_sync, &swapchain);
    Ok(())
}


fn render_submit(
	mut frame_sync: UniqueViewMut<frame_sync::FrameSync>,
	cmd_ctx: UniqueView<command_context::CommandContext>,
	swapchain: UniqueView<swapchain::Swapchain>,
	main_pass: UniqueView<pass::Main>,
	back_pass: UniqueView<pass::Back>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _span = tracy_client::span!();
	_span.emit_color(0x5566AA);

	
	cmd_ctx.execute_commands(&frame_sync, &[&main_pass, &back_pass]);
	cmd_ctx.swapchain_to_present(&frame_sync, &swapchain)?;
	cmd_ctx.end(&frame_sync)?;
	frame_sync.submit(&cmd_ctx)?;
	swapchain.present(&mut frame_sync)?;
	tracy_client::frame_mark();
    Ok(())
}

fn render_wait(
	vulkan_context: UniqueViewMut<vulkan_context::VulkanContext>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    //tracing::debug!("rener: wait");
	vulkan_context.device.wait()?;
    Ok(())
}

fn recreate_swapchain(
	mut swapchain: UniqueViewMut<swapchain::Swapchain>,
	events: UniqueView<modules::core::EventQueue<WindowEvent>>
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _span = tracy_client::span!("recreate_swapchain");

	let new_size = events.events.iter().find_map(|e| match e {
        WindowEvent::Resized(size) => Some(*size),
        _ => None,
    });
	if let Some(new_size) = new_size {
		swapchain.recreate(new_size)?
	}
	
    Ok(())
}

fn setup_renderer(
    world: AllStoragesViewMut,
) -> Result<(), Box<dyn Error + Send + Sync>>  {
    let app_data = world.get_unique::<&AppData>()?;
    let window = world.get_unique::<&Window>()?;
    let size = window.window.inner_size();

    // Create context
    let context = vulkan_context::VulkanContext::new(
        app_data.name,
        app_data.version,
        &window.window
    )?;

    // Create swapchain
    let swapchain = swapchain::Swapchain::new(
        context.instance.clone(),
        context.device.clone(),
        context.surface.clone(),
        size
    )?;

    // Frame sync data
    let frame_sync = frame_sync::FrameSync::new(
        context.device.clone(),
        swapchain.frame_count
    )?;

    // Create command context
    let command_context = command_context::CommandContext::new(
        context.device.clone(),
        swapchain.frame_count,
    )?;

	let main_pass = pass::Main::new(
		context.device.clone(),
		swapchain.frame_count,
		swapchain.format.format,
	)?;
	let back_pass = pass::Back::new(
		context.device.clone(),
		context.allocator.clone(),
		swapchain.frame_count,
		swapchain.format.format,
	)?;

    world.add_unique(context);
    world.add_unique(swapchain);
    world.add_unique(frame_sync);
    world.add_unique(command_context);
	world.add_unique(main_pass);
	world.add_unique(back_pass);
	
    Ok(())
}