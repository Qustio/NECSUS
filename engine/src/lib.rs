use crate::{modules::core::consume_fixed_time, prelude::*};

pub mod prelude;
pub mod modules;
pub use dear_imgui_rs;
pub use nalgebra;
pub use nalgebra_glm;
pub use shipyard;
#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

use hashbrown::HashMap;
use shipyard::{
	Label, Workload, World,
	scheduler::{Label, SystemModificator},
};
use std::error::Error;
use winit::{application::ApplicationHandler, event_loop::EventLoop};
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

use crate::modules::{
	System,
	core::{AppData, EventQueue, EventRegistry},
};

#[derive(DDebug)]
pub struct Engine {
	world: World,
	#[debug(skip)]
	pub systems: Vec<System>,
	pub states: Vec<Box<dyn shipyard::scheduler::Label>>,
	started: bool,
	#[cfg(target_os = "android")]
	app: AndroidApp
}

#[cfg(target_os = "android")]
#[derive(shipyard::Unique, Clone)]
pub struct AndroidAppHandle(pub AndroidApp);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Label, Clone)]
pub enum State {
	Startup,
	PreUpdate,
	Tick,
	Update,
	PostUpdate,
	Cleanup,
}

impl Engine {
	pub fn new(
		name: &'static str,
		version: u32,
		#[cfg(target_os = "android")]
		app: AndroidApp
	) -> Result<Self, Box<dyn Error>> {
		let world = World::new();
		let systems = vec![];
		world.add_unique(AppData { name, version });
		Ok(Self {
			world,
			systems,
			states: vec![
				Box::new(State::Startup),
				Box::new(State::PreUpdate),
				Box::new(State::Update),
				Box::new(State::PostUpdate),
				Box::new(State::Cleanup),
				Box::new(State::Tick),
			],
			started: false,
			#[cfg(target_os = "android")]
			app,
		})
	}

	pub fn register_event<T: Send + Sync + 'static>(&self) {
		self.world.add_unique(EventQueue::<T>::default());
		if self.world.get_unique::<&EventRegistry>().is_err() {
			self.world.add_unique(EventRegistry::default());
		}
		self.world.get_unique::<&mut EventRegistry>().unwrap().push(
			|world: &mut shipyard::World| {
				if let Ok(mut q) = world.get_unique::<&mut EventQueue<T>>() {
					q.clear();
				}
			},
		);
	}

	pub fn run(mut self) -> Result<(), Box<dyn Error>> {
		// Build workloads
		let mut workloads: HashMap<Box<dyn shipyard::scheduler::Label>, Workload> = HashMap::new();

		// init workloads from states
		for state in &self.states {
			let wl = Workload::new(state.dyn_clone());
			workloads.insert(state.dyn_clone(), wl);
		}

		// register systems
		for system in self.systems.drain(..) {
			let state = system.state.dyn_clone();
			let wl = workloads
				.remove(&state)
				.unwrap_or_else(|| Workload::new(state));
			let mut ws = system.workload;

			if let Some(label) = system.label {
				ws = ws.tag(label);
			}
			for after in system.after {
				ws = ws.after_all(after);
			}
			for before in system.before {
				ws = ws.before_all(before);
			}
			workloads.insert(system.state.dyn_clone(), wl.with_system(ws));
		}

		workloads
			.drain()
			.for_each(|(_, wl)| wl.add_to_world(&self.world).unwrap());

		tracing::debug!("{:#?}", self.world.workloads_info());

		// Start event loop
		#[cfg(target_os = "android")]
		let event_loop = EventLoop::builder().with_android_app(self.app.clone()).build().unwrap();
		#[cfg(not(target_os = "android"))]
		let event_loop = EventLoop::builder().build().unwrap();
		event_loop.run_app(&mut self)?;
		Ok(())
	}
}

impl ApplicationHandler for Engine {
	#[tracing::instrument(name = "Engine::resumed", skip_all)]
	
	fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		tracing::info!("resumed");
		let window = modules::window::Window::new(event_loop);
		if self.started {
			// Android destroyed the old ANativeWindow/surface while backgrounded; reattach
			// to the new one without rerunning Startup (which would duplicate spawned
			// entities and rebuild everything, including a second imgui::Context - it's a
			// process-wide singleton that panics on double-init).
			self.world.add_unique(window);
			self.world
				.run_workload(modules::renderer::ReattachSurface)
				.unwrap();
			return;
		}

		self.world.add_unique(window);
		self.world
			.add_unique(modules::components::Camera::default());
		#[cfg(target_os = "android")]
		self.world.add_unique(AndroidAppHandle(self.app.clone()));
		self.world.run_workload(State::Startup).unwrap();
		self.register_event::<winit::event::WindowEvent>();
		self.register_event::<winit::event::DeviceEvent>();
		self.started = true;
	}

	#[tracing::instrument(name = "Engine::window_event", skip_all)]
	fn window_event(
		&mut self,
		event_loop: &winit::event_loop::ActiveEventLoop,
		_: winit::window::WindowId,
		event: winit::event::WindowEvent,
	) {
		if let winit::event::WindowEvent::CloseRequested = event {
			event_loop.exit();
		}
		if let Ok(mut q) = self
			.world
			.get_unique::<&mut EventQueue<winit::event::WindowEvent>>()
		{
			q.push(event);
		}
	}

	#[tracing::instrument(name = "Engine::device_event", skip_all)]
	fn device_event(
		&mut self,
		_: &winit::event_loop::ActiveEventLoop,
		_: winit::event::DeviceId,
		event: winit::event::DeviceEvent,
	) {
		if let Ok(mut q) = self
			.world
			.get_unique::<&mut EventQueue<winit::event::DeviceEvent>>()
		{
			q.push(event);
		}
	}

	#[tracing::instrument(name = "Engine::about_to_wait", skip_all)]
	fn about_to_wait(&mut self, _: &winit::event_loop::ActiveEventLoop) {
		if !self.started {
			return;
		}
		for state in &self.states {
			if state.dyn_eq(&State::Startup) {
				continue;
			}
			if state.dyn_eq(&State::Cleanup) {
				continue;
			}
			if state.dyn_eq(&State::Tick) {
				while self.world.run(consume_fixed_time) {
					self.world.run_workload(State::Tick).unwrap();
				}
				continue;
			}
			self.world.run_workload(state.dyn_clone()).unwrap();
		}		

		// Clear events
		{
			let _span = tracy_client::span!("Clear events");
			let clear_fns = {
				let registry = self.world.get_unique::<&EventRegistry>().unwrap();
				registry.clear_fns().to_vec()
			};

			for clear in clear_fns {
				clear(&mut self.world);
			}
		}

		// Request redraw
		{
			let _span = tracy_client::span!("Request redraw");
			self.world
				.get_unique::<&modules::window::Window>()
				.unwrap()
				.request_redraw();
		}
	}

	#[tracing::instrument(name = "Engine::suspended", skip_all)]
	fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		let _ = event_loop;
	}

	#[tracing::instrument(name = "Engine::exiting", skip_all)]
	fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {
		self.world.run_workload(State::Cleanup).unwrap();
		tracing::info!("Done cleaning");
	}

	#[tracing::instrument(name = "Engine::memory_warning", skip_all)]
	fn memory_warning(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		let _ = event_loop;
	}
}
