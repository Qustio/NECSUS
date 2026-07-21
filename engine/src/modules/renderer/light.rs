use nalgebra_glm::{Mat4, Vec3, Vec4};
use shipyard::Component;

#[derive(Default, Component)]
pub struct DirectionalLight {
	pub position: Vec3,
	pub direction: Vec3,
	pub cast_shadow: bool,
	pub check_outside: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GPULight {
	pub view_proj: Mat4, // light's own view*proj, meaningful only if shadow_layer >= 0
	pub direction: Vec4, // xyz = normalized travel direction, w unused
	pub shadow_layer: i32, // index into the shadow map array, or -1 if unshadowed/overflowed capacity
	pub check_outside: u32,
	pub _pad: [u32; 2],
}
