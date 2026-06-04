use std::{error::Error, sync::Arc};

use shipyard::{scheduler::IntoWorkloadSystem, *};
use winit::{event::{DeviceEvent, KeyEvent, WindowEvent}, keyboard::{KeyCode, PhysicalKey::Code}, window::Fullscreen};

use crate::{Engine, State, modules::{Module, System, renderer::imgui::UiDrawList}, *};

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
		if let winit::event::WindowEvent::KeyboardInput { 
            device_id,
            event: KeyEvent{
                physical_key: Code(s),
                logical_key,
                text,
                location,
                state: winit::event::ElementState::Pressed,
                repeat: false,
                ..
            },
            is_synthetic
        } = &event {
            tracing::info!(target: "input", "fullscreen");
  			match s {
  				KeyCode::F11 => {
  					let mon = window.window.current_monitor().unwrap();
  					let f = window.window.fullscreen();
  					if f.is_none() {
  						window.window.set_fullscreen(Some(Fullscreen::Borderless(Some(mon))));
  					} else {
  						window.window.set_fullscreen(None);
  					}
  				},
  				KeyCode::AltLeft => {
  					window.window.set_cursor_grab(winit::window::CursorGrabMode::Locked);
  				}
  				_ => {}
  			}
        }
	}
    Ok(())
}

fn move_cum(
	events: UniqueView<modules::core::EventQueue<WindowEvent>>,
	device_events: UniqueView<modules::core::EventQueue<DeviceEvent>>,
    mut camera: UniqueViewMut<modules::components::Camera>,
	time: UniqueView<modules::core::Time>,
	window: UniqueViewMut<Window>,
	mut draw_list: UniqueViewMut<UiDrawList>,
) -> Result<(), Box<dyn Error>> {
    let _span = tracy_client::span!();
	let dt = time.elapsed.as_secs_f32();
	for event in events.events.iter() {
		if let winit::event::WindowEvent::KeyboardInput {
			event: KeyEvent{
				physical_key: winit::keyboard::PhysicalKey::Code(key),
				state,
				..
			},
			..
		} = &event {
			camera.process_movement(event, dt);
		}
	}
	for event in device_events.events.iter() {
		camera.process_rotation(event, dt, true);
	}
	let camera_c = camera.clone();
	draw_list.items.push(Box::new(move |ui: &::imgui::Ui| {
		ui.window("Camera")
			.build(|| {
				let pos = camera_c.position();
				let rot = camera_c.rotation();
				ui.text_colored(
					[1.0, 0.0, 0.0, 1.0],
					format!("x: {}", pos.x)
				);
				ui.text_colored(
					[0.0, 1.0, 0.0, 1.0],
					format!("y: {}", pos.y)
				);
				ui.text_colored(
					[0.0, 0.0, 1.0, 1.0],
					format!("z: {}", pos.z)
				);
				ui.text(
					format!("pitch: {}", rot.0)
				);
				ui.text(
					format!("yaw: {}", rot.1)
				);
			});
	}));
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