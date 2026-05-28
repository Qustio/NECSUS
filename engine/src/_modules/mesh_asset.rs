use std::{collections::HashMap, error::Error, path::Path};

use shipyard::*;
use gltf::mesh::util::indices;
use crate::modules::*;

#[derive(Debug, Default, Unique)]
pub(crate) struct AssetManager {
    pub(crate) mesh_assets: HashMap<String, MeshAsset>,
}

#[derive(Debug, Component)]
pub(crate) struct MeshAsset {
    pub(crate) surfaces: Vec<GeoSurface>,
    pub(crate) mesh_buffers: mesh::GPUMeshBuffers,
}

impl MeshAsset {
    // pub fn new(
    //     engine: &Engine,
    //     verts: &[mesh::Vertex],
    //     inds: &[u32],
    // ) -> Result<Self, Box<dyn Error>> {
    //     let mesh_buffers = engine.upload_mesh(verts, inds)?;
    //     Ok(Self {
    //         surfaces: vec![],
    //         mesh_buffers,
    //     })
    // }
}

#[derive(Debug)]
struct GeoSurface {
    start_index: usize,
    count: usize,
}
