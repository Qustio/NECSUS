pub mod vulkan_context;
pub mod window;
pub mod events;
pub mod renderer;
pub mod swapchain;
// pub mod renderable;
// pub mod buffer;
pub mod mesh;
// pub mod mesh_asset;
//pub mod gui;
pub mod components;
pub mod pipeline;
pub mod material;
pub mod core;
pub mod texture;

use std::error::Error;

use shipyard::{scheduler::WorkloadSystem, Component};

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
    pub state: State,
    pub workload: WorkloadSystem
}

impl System {
    pub fn new(state: State, workload: WorkloadSystem) -> Self {
        Self { state, workload }
    }
}