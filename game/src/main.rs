use std::{env, error::Error};

use color_eyre::owo_colors::OwoColorize;
use engine::modules::components::Transform;
use engine::modules::renderer::mesh::{MeshAssetManager, MeshHandle, Vertex};
use engine::nalgebra_glm::Vec3;
use engine::shipyard::{EntitiesViewMut, ViewMut};
use tracing_subscriber::layer::SubscriberExt;
use engine::*;
use engine::modules::{Module, System, renderer::imgui::{UiDrawList, UiDrawable}};
use shipyard::{UniqueViewMut, scheduler::IntoWorkloadSystem};
use tracing::{self, level_filters::LevelFilter};
use tracing_subscriber::{prelude::*, fmt};

struct GameModule;
impl Module for GameModule {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error>> {
        engine.systems.push(
            System::new(Box::new(State::Update), draw_ui.into_workload_system()?)
        );
		engine.systems.push(
            System::new(Box::new(State::Startup), spawn_objects.into_workload_system()?)
        );
        Ok(())
    }
}

struct Demo;
impl UiDrawable for Demo {
    fn draw(&self, ui: &imgui::Ui) {
        ui.show_demo_window(&mut true);
    }
}

fn draw_ui(mut draw_list: UniqueViewMut<UiDrawList>) {
    draw_list.items.push(Box::new(Demo));
}

fn spawn_objects(
	mut entities: EntitiesViewMut,
	mut mesh_assets: UniqueViewMut<MeshAssetManager>,
	mut mesh_handles: ViewMut<MeshHandle>,
	mut transform: ViewMut<Transform>,
) {
	let mut d = Transform::default();
	d.local = nalgebra_glm::translation(&Vec3::new(5.0, 0.0, 0.0));
    mesh_assets.load_gltf::<Vertex>("suzanne.glb").unwrap();
	mesh_assets.load_gltf::<Vertex>("cube.glb").unwrap();
	entities.add_entity(
		(&mut mesh_handles, &mut transform),
		(MeshHandle("Cube.0".to_string()), d)
	);
}



#[cfg(any(feature = "memory_profiling", debug_assertions))]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System>  =
    tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

fn main() -> Result<(), Box<dyn Error>> {


    let subscriber = tracing_subscriber::registry();

    #[cfg(debug_assertions)]
    let subscriber = subscriber.with(tracing_tracy::TracyLayer::default());

    let fmt_layer = fmt::Layer::default()
            .with_file(true)
            .with_ansi(true)
            .with_level(true)
            .with_line_number(true)
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true);
    #[cfg(not(debug_assertions))]
    let fmt_layer = fmt_layer
        .with_filter(tracing_subscriber::filter::LevelFilter::from_level(tracing::Level::ERROR));
    let subscriber = subscriber.with(fmt_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("setup tracy layer");

    color_eyre::install()?;
    Engine::new("test_game", 1)?
        .import::<modules::core::CoreModule>()?
        .import::<modules::events::EventsModule>()?
        .import::<modules::window::WindowModule>()?
        .import::<modules::renderer::RendererModule>()?
        .import::<GameModule>()?
        .run()?;

	tracing::info!("close");
    Ok(())
}
