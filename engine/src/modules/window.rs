use std::{error::Error, sync::Arc};

use shipyard::{scheduler::IntoWorkloadSystem, *};
use winit::{event::{KeyEvent, WindowEvent}, keyboard::KeyCode, window::Fullscreen};

use crate::{modules::{Module, System}, Engine, State, *};

#[derive(Debug)]
pub struct WindowModule;


impl Module for WindowModule {
    fn build(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error>> {
		engine.systems.push(System::new(
            Box::new(State::Update), move_cum.into_workload_system()?)
        );
        engine.systems.push(System::new(
            Box::new(State::Update), read_event.into_workload_system()?)
        );
        Ok(())
    }
}

fn read_event(
    events: UniqueView<modules::core::EventQueue<WindowEvent>>,
    window: UniqueView<Window>,
) -> Result<(), Box<dyn Error>> {
    let _span = tracy_client::span!("read_event");
	for event in events.events.iter() {
		match &event {
        winit::event::WindowEvent::KeyboardInput { 
            device_id,
            event: KeyEvent{
                physical_key: winit::keyboard::PhysicalKey::Code(KeyCode::F11),
                logical_key,
                text,
                location,
                state: winit::event::ElementState::Pressed,
                repeat: false,
                ..
            },
            is_synthetic
        } => {
            tracing::info!(target: "input", "fullscreen");
            let mon = window.window.current_monitor().unwrap();
            let f = window.window.fullscreen();
            if f.is_none() {
                window.window.set_fullscreen(Some(Fullscreen::Borderless(Some(mon))));
            } else {
                window.window.set_fullscreen(None);
            }
            
        }
        _ => ()
    }
	}
    Ok(())
}

fn move_cum(
	events: UniqueView<modules::core::EventQueue<WindowEvent>>,
    mut camera: UniqueViewMut<modules::components::Camera>,
) -> Result<(), Box<dyn Error>> {
    let _span = tracy_client::span!("move_cum");
	for event in events.events.iter() {
		match &event {
			winit::event::WindowEvent::KeyboardInput { 
				device_id,
				event: KeyEvent{
					physical_key: winit::keyboard::PhysicalKey::Code(KeyCode::KeyW),
					logical_key,
					text,
					location,
					state,
					repeat: false,
					..
				},
				is_synthetic
			} => {
				tracing::info!(target: "input", "W");
				tracing::error!(target: "camera","W");
				// dont forget deltatime
				camera.postition.x += 0.1;
				camera.update();
			}
			_ => ()
		}
	}
    Ok(())
}

#[derive(Debug, Unique)]
pub struct Window{
    pub window: Arc<winit::window::Window>
}


impl Window {
    pub fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        let window = event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_title("UWUEngine")
                    .with_decorations(true)
                    .with_transparent(true),
            )
            .expect("Can't create window");
        Self { window: Arc::new(window) }
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }
}