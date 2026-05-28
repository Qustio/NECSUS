use bytemuck::{Pod, Zeroable};
use shipyard::*;
use nalgebra::{Matrix4, Point, Point3, Unit, UnitQuaternion};
use nalgebra_glm::*;

#[derive(Debug, Component, Clone, Copy)]
#[repr(C)]
pub struct Transform {
    pub local: Mat4,
    pub world: Mat4,
}

impl Default for Transform {
	fn default() -> Self {		
		Self {
			local: Matrix4::from_row_slice(&[
				1.0, 0.0, 0.0, 0.0,
				0.0, 0.5, 0.0, 0.0,
				0.0, 0.0, 1.0, 0.0,
				0.0, 0.0, 0.0, 1.0
			]),
			world: Matrix4::from_row_slice(&[
				1.0, 1.0, 1.0, 0.0,
				1.0, 1.0, 1.0, 0.0,
				1.0, 1.0, 1.0, 0.0,
				1.0, 1.0, 1.0, 0.0
			]),
		}
	}
}

#[derive(Debug, Unique, Clone)]
pub struct Camera {
	pub velocity: Vec3,
	pub postition: Point<f32, 3>,

	pub pitch: f32,
	pub yaw: f32,
	pub pitchd: f32,
	pub yawd: f32,
}

impl Default for Camera {
	fn default() -> Self {
		Self {
			velocity: Vec3::default(),
			postition: Point::<f32, 3>::new(0.0, 0.0, 2.0),
			pitch: 0.0,
			yaw: 0.0,
			pitchd: 0.0,
			yawd: 0.0
		}
	}
}

impl Camera {
	pub fn update(&mut self) {
		//glm::mat4 cameraRotation = getRotationMatrix();
    	//position += glm::vec3(cameraRotation * glm::vec4(velocity * 0.5f, 0.f));
		self.pitch += self.pitchd;
		self.yaw += self.yawd;
		let camera_rotation = self.rotation_matrix();
		let step = self.velocity * 0.5;
		let step4d = step.to_homogeneous();
		let r = camera_rotation * step4d;
		let rvec3 = r.xyz();
		self.postition += rvec3;
	}

	pub fn view_matrix(&self) -> Mat4 {
		// to create a correct model view, we need to move the world in opposite
		// direction to the camera
		//  so we will create the camera model matrix and invert
		//glm::mat4 cameraTranslation = glm::translate(glm::mat4(1.f), position);
		//glm::mat4  = getRotationMacameraRotationtrix();
		let camera_translation = Mat4::identity().append_translation(&self.postition.coords);
		let camera_rotation = self.rotation_matrix();
		//return glm::inverse(cameraTranslation * cameraRotation);
		(camera_translation * camera_rotation).try_inverse().unwrap()
	}

	fn rotation_matrix(&self) -> Mat4 {
		// fairly typical FPS style camera. we join the pitch and yaw rotations into
    	// the final rotation matrix

		// glm::quat pitchRotation = glm::angleAxis(pitch, glm::vec3 { 1.f, 0.f, 0.f });
		// glm::quat yawRotation = glm::angleAxis(yaw, glm::vec3 { 0.f, -1.f, 0.f });

		// return glm::toMat4(yawRotation) * glm::toMat4(pitchRotation);
		let pitch_axis = Unit::new_unchecked(Vec3::x_axis());
		let yaw_axis = Unit::new_unchecked(-Vec3::y_axis());
		let pitch_rotation = UnitQuaternion::from_axis_angle(&pitch_axis, self.pitch);
		let yaw_rotation = UnitQuaternion::from_axis_angle(&yaw_axis, self.yaw);

		(yaw_rotation * pitch_rotation).into()
	}
}

