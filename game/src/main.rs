use std::error::Error;

use engine::modules::components::{DirectionalLight, Transform};
use engine::modules::core::Time;
use engine::modules::renderer::material::{MaterialHandle, MaterialManager};
use engine::modules::renderer::mesh::{MeshAssetManager, MeshHandle, Vertex};
use engine::modules::{
	Module, System,
	renderer::imgui::{UiDrawList, UiDrawable},
};
use engine::nalgebra_glm::Vec3;
use engine::shipyard::{EntitiesViewMut, ViewMut};
use engine::*;
use shipyard::{UniqueView, UniqueViewMut, scheduler::IntoWorkloadSystem};
use tracing::{self};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt;

struct GameModule;
impl Module for GameModule {
	fn build(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error>> {
		engine.systems.push(System::new(
			Box::new(State::Update),
			draw_ui.into_workload_system()?,
		));
		engine.systems.push(System::new(
			Box::new(State::Startup),
			spawn_objects.into_workload_system()?,
		));
		engine.systems.push(System::new(
			Box::new(State::Update),
			rotate_light.into_workload_system()?,
		));
		Ok(())
	}
}

fn rotate_light(time: UniqueView<Time>, mut light: UniqueViewMut<DirectionalLight>) {
	let dt = time.delta.as_secs_f32();
	let speed = 0.5;
	light.position = nalgebra_glm::rotate_vec3(&light.position, speed * dt, &Vec3::y());
	light.direction = -light.position.normalize();
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
	mut material_handle: ViewMut<MaterialHandle>,
	mut mesh_handles: ViewMut<MeshHandle>,
	mut transform: ViewMut<Transform>,
) {
	mesh_assets.load_gltf::<Vertex>("suzanne.glb").unwrap();
	mesh_assets.load_gltf::<Vertex>("cube.glb").unwrap();
	entities.add_entity(
		(&mut mesh_handles, &mut material_handle, &mut transform),
		(
			MeshHandle("Cube.001.0".to_string()),
			MaterialHandle("standart".to_string()),
			Transform{
				local: nalgebra_glm::translation(&Vec3::new(5.0, 0.0, 0.0)),
				..Default::default()
			}
		),
	);
	entities.add_entity(
		(&mut mesh_handles, &mut material_handle, &mut transform),
		(
			MeshHandle("Plane.0".to_string()),
			MaterialHandle("standart".to_string()),
			Transform{
				local: nalgebra_glm::translation(&Vec3::new(0.0, -1.0, 0.0)),
				..Default::default()
			}
		),
	);
	entities.add_entity(
		(&mut mesh_handles, &mut material_handle, &mut transform),
		(
			MeshHandle("Suzanne.0".to_string()),
			MaterialHandle("standart".to_string()),
			Transform::default()
		),
	);
}

#[cfg(any(feature = "tracy"))]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System> =
	tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

fn main() -> Result<(), Box<dyn Error>> {
	let subscriber = tracing_subscriber::registry();

	#[cfg(feature = "tracy")]
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
	let fmt_layer = fmt_layer.with_filter(tracing_subscriber::filter::LevelFilter::from_level(
		tracing::Level::ERROR,
	));
	let subscriber = subscriber.with(fmt_layer);

	tracing::subscriber::set_global_default(subscriber).expect("setup tracy layer");

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
