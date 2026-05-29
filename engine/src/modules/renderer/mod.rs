pub mod vulkan_context;
pub mod swapchain;
pub mod frame_sync;

use std::error::Error;
use shipyard::{AllStoragesViewMut, Label, scheduler::IntoWorkloadTrySystem};
use crate::{State, modules::{Module, System, core::AppData, window::Window}};

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
            System::new(Box::new(Render), render_submit.into_workload_try_system()?)
            .after("Record")
        );
        engine.systems.push(
            System::new(Box::new(Render), render_record.into_workload_try_system()?)
            .label("Record")
        );
        Ok(())
    }
}

fn render_record(

) -> Result<(), Box<dyn Error + Send + Sync>> {
    //tracing::debug!("rener: record");
    Ok(())
}

fn render_submit(

) -> Result<(), Box<dyn Error + Send + Sync>> {
    //tracing::debug!("rener: submit");
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
    ).unwrap();

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

    world.add_unique(context);
    world.add_unique(swapchain);
    world.add_unique(frame_sync);
    Ok(())
}