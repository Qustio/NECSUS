use std::{
    time::{Duration, Instant},
};

use shipyard::{scheduler::IntoWorkloadSystem, *};
use winit::{event::KeyEvent, keyboard::NamedKey, monitor::VideoModeHandle, window::Fullscreen};

use crate::{
    Engine, State,
    modules::{Module, System, events::Event},
};

#[derive(Debug)]
pub struct CoreModule;

impl Module for CoreModule {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error>> {
        engine.systems.push(System::new(
            Box::new(State::Startup), startup.into_workload_system()?
        ));
        // why it crashes if uncomment?
        //engine.systems.push(System::new(State::PreUpdate, update_fixed_time.into_workload_system()?));
        engine.systems.push(System::new(
            Box::new(State::Update),
            update_time.into_workload_system()?,
        ));
        Ok(())
    }
}

#[derive(Unique)]
pub struct EventQueue<T: Send + Sync + 'static> {
    pub events: Vec<T>,
}

impl<T: Send + Sync + 'static> Default for EventQueue<T> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl<T: Send + Sync + 'static> EventQueue<T> {
    pub fn push(&mut self, event: T) {
        self.events.push(event);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[derive(Unique, Default)]
pub struct EventRegistry {
    clear_fns: Vec<fn(&mut shipyard::World)>,
}

impl EventRegistry {
    pub fn push(&mut self, f: fn(&mut shipyard::World)) {
        self.clear_fns.push(f);
    }

    pub fn clear_fns(&self) -> &[fn(&mut World)] {
        &self.clear_fns
    }
}

#[derive(Unique)]
pub struct FixedTime {
    pub step: Duration,
    pub accumulator: Duration,
}

#[derive(Unique)]
pub struct AppData {
    pub name: &'static str,
    pub version: u32,
}

#[derive(Unique)]
pub struct Time {
    pub delta: Duration,
    pub elapsed: Duration,
    pub last_frame: Instant,
}

fn startup(world: AllStoragesViewMut) {
    //world.add_unique(EventRegistry::default());
    world.add_unique(Time {
        delta: Duration::ZERO,
        elapsed: Duration::ZERO,
        last_frame: Instant::now(),
    });
    world.add_unique(FixedTime {
        step: Duration::from_millis(1000),
        accumulator: Duration::ZERO,
    });
}

fn update_fixed_time(time: UniqueView<Time>, mut fixed: UniqueViewMut<FixedTime>) {
    fixed.accumulator += time.delta;
}

fn update_time(mut time: UniqueViewMut<Time>) {
    time.elapsed = time.last_frame.elapsed();
    time.last_frame = Instant::now();
}
