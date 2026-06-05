use std::{error::Error, sync::Arc};

use shipyard::{scheduler::IntoWorkloadSystem, *};
use winit::{
	event::{DeviceEvent, KeyEvent, WindowEvent},
	keyboard::KeyCode,
	window::Fullscreen,
};

use crate::{
	Engine, State,
	modules::{Module, System, renderer::imgui::UiDrawList},
	*,
};

#[derive(Debug)]
pub struct WindowModule;

impl Module for WindowModule {
	fn build(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error>> {
		engine.systems.push(System::new(
			Box::new(State::Update),
			move_cum.into_workload_system()?,
		));
		engine.systems.push(System::new(
			Box::new(State::Update),
			read_event.into_workload_system()?,
		));
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
			device_id: _,
			event:
				KeyEvent {
					physical_key: winit::keyboard::PhysicalKey::Code(KeyCode::F11),
					logical_key: _,
					text: _,
					location: _,
					state: winit::event::ElementState::Pressed,
					repeat: false,
					..
				},
			is_synthetic: _,
		} = &event
		{
			tracing::info!(target: "input", "fullscreen");
			let mon = window.window.current_monitor().unwrap();
			let f = window.window.fullscreen();
			if f.is_none() {
				window
					.window
					.set_fullscreen(Some(Fullscreen::Borderless(Some(mon))));
			} else {
				window.window.set_fullscreen(None);
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
	let dt = time.delta.as_secs_f32();
	let speed = 5.0;
	let sens = 0.002;
	for event in events.events.iter() {
		if let winit::event::WindowEvent::KeyboardInput {
			event:
				KeyEvent {
					physical_key: winit::keyboard::PhysicalKey::Code(key),
					state,
					repeat: false,
					..
				},
			..
		} = &event
		{
			let pressed = *state == winit::event::ElementState::Pressed;
			match key {
				KeyCode::KeyW => camera.velocity.z = if pressed { 1.0 } else { 0.0 },
				KeyCode::KeyS => camera.velocity.z = if pressed { -1.0 } else { 0.0 },
				KeyCode::KeyA => camera.velocity.x = if pressed { -1.0 } else { 0.0 },
				KeyCode::KeyD => camera.velocity.x = if pressed { 1.0 } else { 0.0 },
				KeyCode::AltLeft => {
					if !pressed {
						window
							.window
							.set_cursor_grab(winit::window::CursorGrabMode::Locked)
							.ok();
						window.window.set_cursor_visible(false);
					}
				}
				_ => {}
			}
		}
	}
	for event in device_events.events.iter() {
		if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
			camera.yawd = -*dx as f32 * sens;
			camera.pitchd = *dy as f32 * sens;
		}
	}

	let scaled = camera.velocity * speed * dt;
	let saved = camera.velocity;
	camera.velocity = scaled;
	camera.update();
	camera.velocity = saved;
	camera.pitchd = 0.0;
	camera.yawd = 0.0;
	let camera_c = camera.clone();
	draw_list.items.push(Box::new(move |ui: &::imgui::Ui| {
		ui.window("Camera").build(|| {
			ui.text_colored([1.0, 0.0, 0.0, 1.0], format!("x: {}", camera_c.postition.x));
			ui.text_colored([0.0, 1.0, 0.0, 1.0], format!("y: {}", camera_c.postition.y));
			ui.text_colored([0.0, 0.0, 1.0, 1.0], format!("z: {}", camera_c.postition.z));
			ui.text(format!("pitch: {}", camera_c.pitch));
			ui.text(format!("yaw: {}", camera_c.yaw));
		});
	}));
	Ok(())
}

#[derive(Debug, Unique)]
pub struct Window {
	pub window: Arc<winit::window::Window>,
}

impl Window {
	pub fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
		let window = event_loop
			.create_window(
				winit::window::Window::default_attributes()
					.with_title("NECSUS")
					.with_decorations(true)
					.with_transparent(true),
			)
			.expect("Can't create window");
		Self {
			window: Arc::new(window),
		}
	}

	pub fn request_redraw(&self) {
		self.window.request_redraw();
	}
}
