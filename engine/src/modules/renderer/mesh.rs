use std::mem::offset_of;
use std::{error::Error, sync::Arc};

use ash::*;
use hashbrown::HashMap;
use nalgebra::allocator;
use shipyard::{Component, Unique};

use super::buffer::Buffer;
use super::vulkan_context::Allocator;

pub trait MeshData: Send + Sync {
	fn vertex_buffer(&self) -> vk::Buffer;
	fn index_buffer(&self) -> vk::Buffer;
	fn vertex_count(&self) -> u32;
	fn index_count(&self) -> u32;
}

#[derive(Unique)]
pub struct MeshAssetManager {
	pub mesh_assets: HashMap<String, Box<dyn MeshData>>,
	allocator: Arc<Allocator>,
}

#[derive(Component)]
pub struct MeshHandle(pub String);

impl MeshAssetManager {
	pub(super) fn new(allocator: Arc<Allocator>) -> Result<Self, Box<dyn Error + Send + Sync>> {
		Ok(Self {
			mesh_assets: HashMap::new(),
			allocator,
		})
	}
	pub fn load_gltf<V: FromGLTF + 'static>(
		&mut self,
		path: &str,
	) -> Result<(), Box<dyn Error + Send + Sync>> {
		let (doc, buffers, _) = gltf::import(path)?;
		for mesh in doc.meshes() {
			for (i, primitive) in mesh.primitives().enumerate() {
				let (vertexes, indices) = V::from_primitive(&primitive, &buffers);
				let key = format!("{}.{}", mesh.name().unwrap_or("mesh"), i);
				tracing::debug!("Loaded mesh: {}", key);
				self.mesh_assets.insert(
					key,
					Box::new(Mesh::<V>::new(vertexes, indices, self.allocator.clone())?),
				);
			}
		}
		Ok(())
	}
}

pub struct Mesh<V> {
	pub(crate) vertex_buffer: Buffer<V>,
	pub(crate) index_buffer: Buffer<u32>,
	pub(crate) vertex_count: u32,
	pub(crate) index_count: u32,
}

impl<V> Mesh<V> {
	fn new(
		vertexes: Vec<V>,
		indicies: Vec<u32>,
		allocator: Arc<Allocator>,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let vertex_buffer = Buffer::<V>::from_slice(
			allocator.clone(),
			&vertexes,
			vk::BufferUsageFlags::VERTEX_BUFFER,
		)?;
		let index_buffer = Buffer::<u32>::from_slice(
			allocator.clone(),
			&indicies,
			vk::BufferUsageFlags::INDEX_BUFFER,
		)?;
		let vertex_count = vertexes.len() as u32;
		let index_count = indicies.len() as u32;
		Ok(Self {
			vertex_buffer,
			index_buffer,
			vertex_count,
			index_count,
		})
	}
}

impl<V: Sync + Send> MeshData for Mesh<V> {
	fn vertex_buffer(&self) -> vk::Buffer {
		self.vertex_buffer.buffer()
	}

	fn index_buffer(&self) -> vk::Buffer {
		self.index_buffer.buffer()
	}

	fn vertex_count(&self) -> u32 {
		self.vertex_count
	}

	fn index_count(&self) -> u32 {
		self.index_count
	}
}

#[repr(C)]
#[derive(Debug)]
pub struct Vertex {
	pub(crate) pos: nalgebra_glm::Vec4,
	pub(crate) normal: nalgebra_glm::Vec3,
	pub(crate) color: nalgebra_glm::Vec4,
}

pub trait VertexDescription {
	fn binding() -> vk::VertexInputBindingDescription;
	fn attributes() -> Vec<vk::VertexInputAttributeDescription>;
}

impl VertexDescription for Vertex {
	fn binding() -> vk::VertexInputBindingDescription {
		vk::VertexInputBindingDescription {
			binding: 0,
			stride: size_of::<Vertex>() as u32,
			input_rate: vk::VertexInputRate::VERTEX,
		}
	}

	fn attributes() -> Vec<vk::VertexInputAttributeDescription> {
		vec![
			vk::VertexInputAttributeDescription::default()
				.location(0)
				.binding(0)
				.format(vk::Format::R32G32B32A32_SFLOAT)
				.offset(offset_of!(Vertex, pos) as u32),
			vk::VertexInputAttributeDescription::default()
				.location(1)
				.binding(0)
				.format(vk::Format::R32G32B32_SFLOAT)
				.offset(offset_of!(Vertex, normal) as u32),
			vk::VertexInputAttributeDescription::default()
				.location(2)
				.binding(0)
				.format(vk::Format::R32G32B32A32_SFLOAT)
				.offset(offset_of!(Vertex, color) as u32),
		]
	}
}

pub trait FromGLTF: Sized + Sync + Send {
	fn from_primitive(
		primitive: &gltf::Primitive,
		buffers: &[gltf::buffer::Data],
	) -> (Vec<Self>, Vec<u32>);
}

impl FromGLTF for Vertex {
	fn from_primitive(
		primitive: &gltf::Primitive,
		buffers: &[gltf::buffer::Data],
	) -> (Vec<Self>, Vec<u32>) {
		let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
		let positions = reader.read_positions().unwrap();
		let normals = reader.read_normals().unwrap();
		let colors: Vec<[f32; 4]> = reader
			.read_colors(0)
			.map(|c| c.into_rgba_f32().collect())
			.unwrap_or_else(|| vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]);
		let vertexes = positions
			.zip(normals)
			.zip(colors)
			.map(|((p, n), c)| Vertex {
				pos: nalgebra_glm::vec4(p[0], p[1], p[2], 1.0),
				normal: nalgebra_glm::vec3(n[0], n[1], n[2]),
				color: nalgebra_glm::vec4(c[0], c[1], c[2], c[3]),
			})
			.collect();
		let indicies = reader
			.read_indices()
			.map(|i| i.into_u32().collect())
			.unwrap();

		(vertexes, indicies)
	}
}
