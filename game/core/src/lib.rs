#[cfg(target_os = "android")]
pub use engine::AndroidApp;
use engine::dear_imgui_rs::Ui;

use std::error::Error;

use engine::modules::components::Transform;
use engine::modules::core::{EventQueue, Time};
use engine::modules::physics::{Collider, ColliderBuilder, RapierData, RigidBody};
use engine::modules::renderer::light::{self, DirectionalLight};
use engine::modules::renderer::material::MaterialHandle;
use engine::modules::renderer::mesh::{MeshAssetManager, MeshHandle, Vertex};
use engine::modules::{
	Module, System,
	renderer::imgui::{UiDrawable},
};
use engine::nalgebra::UnitQuaternion;
use engine::nalgebra_glm::Vec3;
use engine::shipyard::{EntitiesViewMut, IntoIter, ViewMut};
use engine::*;
use shipyard::{UniqueView, UniqueViewMut, scheduler::IntoWorkloadSystem};
use tracing::{self};
use tracing_subscriber::fmt;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

// tracing_subscriber's fmt::Layer::with_writer wants `Fn() -> W where W: io::Write`;
// wrapping the file lets a cheap Arc clone satisfy that on every log call.
#[cfg(target_os = "android")]
#[derive(Clone)]
struct SharedFile(std::sync::Arc<std::sync::Mutex<std::fs::File>>);

#[cfg(target_os = "android")]
impl std::io::Write for SharedFile {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		self.0.lock().unwrap().write(buf)
	}
	fn flush(&mut self) -> std::io::Result<()> {
		self.0.lock().unwrap().flush()
	}
}

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

fn rotate_light(time: UniqueView<Time>, mut lights: ViewMut<DirectionalLight>) {
	let dt = time.delta.as_secs_f32();
	let speed = 0.5;
	for light in (&mut lights).iter() {
		light.position = nalgebra_glm::rotate_vec3(&light.position, speed * dt, &Vec3::y());
		light.direction = -light.position.normalize();
	}
}

struct Demo;
impl UiDrawable for Demo {
	fn draw(&self, ui: &Ui) {
		ui.show_demo_window(&mut true);
	}
}

fn draw_ui(mut draw_list: UniqueViewMut<EventQueue<Box<dyn UiDrawable>>>) {
	draw_list.push(Box::new(Demo));
}

fn spawn_objects(
	mut entities: EntitiesViewMut,
	mut mesh_assets: UniqueViewMut<MeshAssetManager>,
	mut material_handle: ViewMut<MaterialHandle>,
	mut mesh_handles: ViewMut<MeshHandle>,
	mut transform: ViewMut<Transform>,
	mut light: ViewMut<DirectionalLight>,
	mut phycics: UniqueViewMut<RapierData>,
	mut rigid_bodies: ViewMut<RigidBody>,
	mut colliders: ViewMut<Collider>,
) {
	mesh_assets.load_gltf::<Vertex>("suzanne.glb").unwrap();
	mesh_assets.load_gltf::<Vertex>("cube.glb").unwrap();
	entities.add_entity(
		(&mut light),
		(light::DirectionalLight {
			position: Vec3::new(-10.0, 5.0, 0.0),
			direction: Vec3::new(-4.0, 1.0, 0.0).normalize(),
			cast_shadow: true,
			check_outside: false,
		}),
	);
	entities.add_entity(
		(&mut light),
		(light::DirectionalLight {
			position: Vec3::new(10.0, 5.0, 0.0),
			direction: Vec3::new(4.0, 1.0, 0.0).normalize(),
			cast_shadow: true,
			check_outside: false,
		}),
	);
	{
		let t = Transform {
			translation: Vec3::new(3.5, 10.0, 0.0),
			rotation: UnitQuaternion::from_axis_angle(&Vec3::x_axis(), 35f32.to_radians()),
			..Default::default()
		};
		let (rbh, ch) = phycics.insert_dynamic(
			t,
			ColliderBuilder::cuboid(1.0, 1.0, 1.0)
			//.rotation(t.rotation.scaled_axis().into())
			.mass(10.0)
			.friction(0.05)
			.build()
		);
		entities.add_entity(
			(
				&mut mesh_handles,
				&mut material_handle,
				&mut transform,
				&mut rigid_bodies,
				&mut colliders,
			),
			(
				MeshHandle("Cube.001.0".to_string()),
				MaterialHandle("standart".to_string()),
				t,
				RigidBody(rbh),
				Collider(ch),
			),
		);
	}
	let pt = Transform {
		translation: Vec3::new(0.0, -1.0, 0.0),
		rotation: UnitQuaternion::from_axis_angle(&Vec3::x_axis(), 5f32.to_radians()),
		..Default::default()
	};
	let (rbh, ch) = phycics.insert_fixed(
		pt,
		ColliderBuilder::cuboid(10.0, 0.1, 10.0)
		.friction(0.05)
		
		.build()
	);
	entities.add_entity(
		(
			&mut mesh_handles,
			&mut material_handle,
			&mut transform,
			&mut rigid_bodies,
			&mut colliders,
		),
		(
			MeshHandle("Plane.0".to_string()),
			MaterialHandle("standart".to_string()),
			pt,
			RigidBody(rbh),
			Collider(ch),
		),
	);
	entities.add_entity(
		(&mut mesh_handles, &mut material_handle, &mut transform),
		(
			MeshHandle("Suzanne.0".to_string()),
			MaterialHandle("standart".to_string()),
			Transform::default(),
		),
	);
}

#[cfg(feature = "memory_profiling")]
#[global_allocator]
static GLOBAL: tracy_client::ProfiledAllocator<std::alloc::System> =
	tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

pub fn game_main(
	#[cfg(target_os = "android")]
	app: AndroidApp
) -> Result<(), Box<dyn Error>> {
	let subscriber = tracing_subscriber::registry();

	#[cfg(target_os = "android")]	
	let subscriber = subscriber.with(
		paranoid_android::layer("necsus").with_filter(
			tracing_subscriber::filter::Targets::new()
				.with_default(tracing::Level::TRACE)
				.with_target("winit", tracing::Level::WARN),
		),
	);

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

	#[cfg(target_os = "android")]
	let log_path = app
		.external_data_path()
		.expect("application external data path is none")
		.join("necsus.log");
	#[cfg(target_os = "android")]
	let subscriber = {
		let file = std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(&log_path)
			.expect("couldn't open log file");
		let file = SharedFile(std::sync::Arc::new(std::sync::Mutex::new(file)));
		let file_layer = fmt::Layer::default()
			.with_writer(move || file.clone())
			.with_ansi(false)
			.with_target(true);
		subscriber.with(file_layer)
	};

	// android can re-invoke this entry point in the same process (activity relaunch after
	// a crash, some config-change paths) - the subscriber is a process-wide singleton, so
	// treat "already set" as fine rather than panicking.
	let _ = tracing::subscriber::set_global_default(subscriber);

	#[cfg(target_os = "android")]
	tracing::info!("log file at {:?}", log_path);

	color_eyre::install()?;
	// color_eyre's hook prints to stderr, which doesn't reach necsus.log or the
	// android logcat tracing layer - chain a log through tracing too, so a panic
	// (e.g. the Startup workload's .unwrap()) is visible wherever the log file is.
	let default_panic_hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |info| {
		tracing::error!("PANIC: {}", info);
		default_panic_hook(info);
	}));

	Engine::new(
		"test_game", 
		1,
		#[cfg(target_os = "android")]
		app
	)?
		.import::<modules::core::CoreModule>()?
		.import::<modules::events::EventsModule>()?
		.import::<modules::window::WindowModule>()?
		.import::<modules::renderer::RendererModule>()?
		.import::<modules::physics::PhysicsModule>()?
		.import::<GameModule>()?
		.run()?;

	tracing::info!("close");
	Ok(()) 
}