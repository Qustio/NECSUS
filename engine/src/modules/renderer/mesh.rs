use ash::*;

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Vertex {
    pub(crate) pos: nalgebra_glm::Vec4,
    pub(crate) normal: nalgebra_glm::Vec3,
    pub(crate) color: nalgebra_glm::Vec4,
    pub(crate) ui: nalgebra_glm::Vec2,
}