use bytemuck::{Pod, Zeroable};
use shipyard::*;
use nalgebra::{Matrix4, Point, Point3, Unit, UnitQuaternion};
use nalgebra_glm::*;
use winit::{event::{DeviceEvent::MouseMotion, WindowEvent::KeyboardInput}, keyboard::{KeyCode, PhysicalKey}};

use crate::modules::events;

#[derive(Debug, Component, Clone, Copy)]
#[repr(C)]
pub struct Transform {
    pub local: Mat4,
    pub world: Mat4,
}

impl Default for Transform {
	fn default() -> Self {
		Self {
			local: Matrix4::identity(),
			world: Matrix4::identity(),
		}
	}
}

#[derive(Debug, Unique, Clone, Default)]
pub struct Camera {
	// Spatial positioning and orientation vectors
    // These form the camera's local coordinate system in world space
	position: Vec3, // Camera's location in world coordinates
	front: Vec3, // Forward direction (where camera is looking)
	up: Vec3, // Camera's local up direction (for roll control)
	right: Vec3, // Camera's local right direction (perpendicular to front and up)
	world_up: Vec3, // Global up vector reference (typically Y-axis)

	// Rotation representation using Euler angles
    // Provides intuitive control while managing gimbal lock and other rotation complexities
	yaw: f32, // Horizontal rotation around the world up-axis (left-right looking)
	pitch: f32, // Vertical rotation around the camera's right axis (up-down looking)

	// User interaction and behavior parameters
    // These control how the camera responds to input and environmental factors
	movement_speed: f32, // Units per second for translation movement
	mouse_sensitivity: f32, // Multiplier for mouse input to rotation angle conversion
	zoom: f32, // Field of view control for perspective projection
}

impl Camera {
	pub fn new() -> Self {
		let position = Vec3::new(-4.0, 0.0, 0.0);
		let front = Vec3::x();
		let up = Vec3::y();
		let right = Vec3::z();
		let yaw = -90.0f32;
		let pitch = 0.0f32;
		let zoom = 90f32;
		let mut s = Self {
			position,
			front,
			up,
			right,
			world_up: Vec3::y(),
			yaw,
			pitch,
			zoom,
			movement_speed: 50.0,
			mouse_sensitivity: 0.1,
		};
		s.update_camera_vectors();
		s
	}
	pub fn process_movement(&mut self, event: &winit::event::WindowEvent, delta_time: f32) {
		let velocity = self.movement_speed * delta_time;
		let mut direction = Vec3::zeros();
		if let KeyboardInput { device_id: _, event, is_synthetic: _ } = event {
			if !event.state.is_pressed() {
				return;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::KeyW) {
				direction += self.front;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::KeyS) {
				direction -= self.front;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::KeyA) {
				direction += self.right;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::KeyD) {
				direction -= self.right;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::Space) {
				direction += self.up;
			}
			if event.physical_key == PhysicalKey::Code(KeyCode::ShiftLeft) {
				direction -= self.up;
			}
		}
		self.position += direction * velocity;
	}
	pub fn process_rotation(&mut self, event: &winit::event::DeviceEvent, delta_time: f32, constrain_pitch: bool) {
		if let MouseMotion{delta: (dx, dy)} = event {
			self.yaw += *dx as f32 * self.mouse_sensitivity;
			self.pitch += *dy as f32 * self.mouse_sensitivity;
			if constrain_pitch {
				self.pitch = self.pitch.clamp(-89.0, 89.0);
			}
			// Update camera vectors based on updated Euler angles
			self.update_camera_vectors();
		}
	}
	pub fn process_zoom(event: winit::event::DeviceEvent, delta_time: f32) {
		
	}
	pub fn position(&self) -> Vec3 {
		self.position
	}
	pub fn rotation(&self) -> (f32, f32) {
		(self.pitch, self.yaw)
	}
	pub fn front(&self) -> Vec3 {
		self.front
	}
	pub fn zoom(&self) -> f32 {
		self.zoom
	}
	pub fn view_matrix(&self) -> Mat4 {
		tracing::info!("camera: {:#?}", self);
		nalgebra_glm::look_at(&self.position, &(&self.position + &self.front), &self.up)
		//nalgebra_glm::translation(&Vec3::new(0.0, 0.0, -4.0))
	}
	pub fn projection_matrix(&self, aspect_ratio: f32, near_plane: f32, far_plane: f32) -> Mat4 {
		nalgebra_glm::perspective_zo(aspect_ratio, self.zoom.to_radians(), near_plane, far_plane)
	}
	// Internal coordinate system maintenance
    // Ensures mathematical consistency when orientation changes occur
	fn update_camera_vectors(&mut self) {
		// Calculate the new front vector
		let new_front = Vec3::new(
			self.yaw.to_radians().cos() * self.pitch.to_radians().cos(),
			self.pitch.to_radians().sin(),
			self.yaw.to_radians().sin() * self.pitch.to_radians().cos(),
		);
		self.front = new_front.normalize();

		// Recalculate the right and up vectors
		self.right = self.front.cross(&self.world_up).normalize();
		self.up = self.right.cross(&self.front).normalize();
	}
}

