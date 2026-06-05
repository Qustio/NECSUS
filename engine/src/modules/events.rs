use shipyard::*;

use crate::modules::*;

pub struct EventsModule;

impl Module for EventsModule {
	fn build(_engine: &mut crate::Engine) -> Result<(), Box<dyn std::error::Error>> {
		Ok(())
	}
}

#[derive(Debug, Unique)]
pub struct Event {
	pub event: winit::event::WindowEvent,
}

impl Event {
	pub fn new(event: winit::event::WindowEvent) -> Self {
		Self { event }
	}
}
