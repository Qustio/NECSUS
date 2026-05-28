pub mod window;
pub mod events;
// pub mod renderer;
// pub mod swapchain;
pub mod renderer;
// pub mod buffer;
// pub mod mesh;
// pub mod mesh_asset;
// pub mod gui;
pub mod components;
// pub mod pipeline;
// pub mod material;
pub mod core;
// pub mod texture;

use std::error::Error;

use shipyard::{Component, scheduler::{Label, WorkloadSystem}};

use crate::{Engine, State};

pub trait Module {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn Error>>;
}

impl Engine {
    pub fn import<T: Module>(mut self) -> Result<Self, Box<dyn Error>> {
        T::build(&mut self)?;
        Ok(self)
    }
}

#[derive(Component)]
pub struct System {
    pub state: Box<dyn shipyard::scheduler::Label>,
    pub workload: WorkloadSystem,
    pub label: Option<Box<dyn Label>>,
    pub after: Vec<Box<dyn Label>>,
    pub before: Vec<Box<dyn Label>>,
}

impl System {
    pub fn new(state: Box<dyn shipyard::scheduler::Label>, workload: WorkloadSystem) -> Self {
        Self {
            state,
            workload,
            label: None,
            after: vec![],
            before: vec![]
        }
    }

    pub fn label(mut self, label: impl Label + 'static) -> Self {
        self.label = Some(Box::new(label));
        self
    }

    pub fn before(mut self, label: impl Label + 'static) -> Self {
        self.before.push(Box::new(label));
        self
    }

    pub fn after(mut self, label: impl Label + 'static) -> Self {
        self.after.push(Box::new(label));
        self
    }
}