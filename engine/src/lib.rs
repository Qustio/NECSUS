pub mod modules;

use std::error::Error;
use hashbrown::HashMap;
use shipyard::{Label, Workload, World, scheduler::{Label, SystemModificator}};
use winit::{application::ApplicationHandler, event_loop::EventLoop};

use crate::modules::{System, core::{AppData, EventQueue, EventRegistry}, renderer::Render};

pub struct Engine {
    world: World,
    pub systems: Vec<System>,
    pub states: Vec<Box<dyn shipyard::scheduler::Label>>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Label, Clone)]
pub enum State {
    Startup,
    PreUpdate,
    Update,
    PostUpdate,
    Cleanup,
}

impl Engine {
    pub fn new(name: &'static str, version: u32) -> Result<Self, Box<dyn Error>> {
        let world = World::new();
        let systems = vec![];
        world.add_unique(AppData{
            name, version
        });
        Ok(Self{
            world,
            systems,
            states: vec![
                Box::new(State::Startup),
                Box::new(State::PreUpdate),
                Box::new(State::Update),
                Box::new(State::PostUpdate),
                Box::new(State::Cleanup),
            ]
        })
    }

    pub fn register_event<T: Send + Sync + 'static>(&self) {
        self.world
            .add_unique(EventQueue::<T>::default());
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
            let wl = workloads.remove(&state).unwrap_or_else(|| Workload::new(state));
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
        let event_loop = EventLoop::builder()
            .build()
            .unwrap();
        event_loop.run_app(&mut self)?;
        Ok(())
    }
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = modules::window::Window::new(event_loop);
        
        self.world.add_unique(window);
        self.world
            .add_unique(modules::components::Camera::default());
        self.world.run_workload(State::Startup).unwrap();
        self.register_event::<winit::event::WindowEvent>();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let mut window_events = self
            .world
            .get_unique::<&mut modules::core::EventQueue<winit::event::WindowEvent>>()
            .unwrap();
        window_events.push(event.clone());
        if let winit::event::WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _span = tracy_client::span!("EventLoop");
        for state in &self.states {
            if state.dyn_eq(&State::Startup) { continue; }
            self.world.run_workload(state.dyn_clone()).unwrap();
        }
        // Clear events
        let clear_fns = {
            let registry = self.world.get_unique::<&EventRegistry>().unwrap();
            registry.clear_fns().to_vec()
        };

        for clear in clear_fns {
            clear(&mut self.world);
        }

        // Request redraw
        self.world
            .get_unique::<&modules::window::Window>()
            .unwrap()
            .request_redraw();
    }

    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }

    fn exiting(&mut self, e: &winit::event_loop::ActiveEventLoop) {
        self.world.run_workload(State::Cleanup);
        tracing::info!("Done cleaning");
    }

    fn memory_warning(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let _ = event_loop;
    }
}
