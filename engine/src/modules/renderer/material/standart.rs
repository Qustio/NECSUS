use std::sync::Mutex;

use bytemuck::bytes_of;
use nalgebra_glm::{Mat4, Vec3};
use shipyard::{IntoIter, View};

use crate::modules::{
	components,
	renderer::{
		buffer::Buffer,
		gbuffers::{GBuffers, MAX_SHADOW_CASTERS},
		light::{DirectionalLight, GPULight},
		mesh::{Vertex, VertexDescription},
		swapchain::Swapchain,
	},
};

use super::*;

pub struct StandartMaterial {
	shader: vk::ShaderModule,
	color_format: vk::Format,
	depth_format: vk::Format,
	shadow_format: vk::Format,
	shadow_sampler: vk::Sampler,
	frame_set_layout: vk::DescriptorSetLayout,
	desc_sets: Vec<vk::DescriptorSet>,
	desc_set_layout: vk::DescriptorSetLayout,
	desc_pool: vk::DescriptorPool,
	device: Arc<Device>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawConstants {
	model: Mat4,
	light_view_proj: Mat4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct CameraUniformData {
	pub view: Mat4,
	pub proj: Mat4,
	pub view_proj: Mat4,
	pub time: f32,
	pub light_count: u32,
	pub _pad0: u32,
	pub _pad1: u32,
}

pub(super) const MAX_LIGHTS: usize = 16;

/// Tells the shadow pass which lights to render and where: `layer` indexes into
/// the shadow map array, `view_proj` is that light's own view*proj.
#[derive(Clone, Copy)]
pub(in super::super) struct ShadowCasterInfo {
	pub layer: u32,
	pub view_proj: Mat4,
}

#[derive(shipyard::Unique)]
pub struct FrameUniforms {
	camera_buffers: Vec<Buffer<CameraUniformData>>, // per frame-in-flight
	light_buffers: Vec<Buffer<GPULight>>,           // per frame-in-flight, capacity MAX_LIGHTS
	shadow_casters: Mutex<Vec<ShadowCasterInfo>>,   // recomputed + consumed within the same frame
	desc_sets: Vec<vk::DescriptorSet>,
	desc_set_layout: vk::DescriptorSetLayout,
	desc_pool: vk::DescriptorPool,
	device: Arc<Device>,
}

impl FrameUniforms {
	pub(in super::super) fn new(
		device: Arc<Device>,
		allocator: Arc<Allocator>,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let device = device.clone();

		let camera_buffers: Vec<Buffer<CameraUniformData>> = (0..frame_count)
			.map(|_| Buffer::<CameraUniformData>::new_uniform(allocator.clone()))
			.collect::<Result<Vec<_>, _>>()?;

		let light_buffers: Vec<Buffer<GPULight>> = (0..frame_count)
			.map(|_| Buffer::<GPULight>::new_storage(allocator.clone(), MAX_LIGHTS))
			.collect::<Result<Vec<_>, _>>()?;

		let desc_set_layout = unsafe {
			let bindings = [
				vk::DescriptorSetLayoutBinding::default()
					.binding(0)
					.descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
					.descriptor_count(1)
					.stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
				vk::DescriptorSetLayoutBinding::default()
					.binding(1)
					.descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
					.descriptor_count(1)
					.stage_flags(vk::ShaderStageFlags::FRAGMENT),
			];
			device.create_descriptor_set_layout(
				&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
				None,
			)?
		};
		let desc_pool = unsafe {
			let pool_sizes = [
				vk::DescriptorPoolSize::default()
					.ty(vk::DescriptorType::UNIFORM_BUFFER)
					.descriptor_count(frame_count),
				vk::DescriptorPoolSize::default()
					.ty(vk::DescriptorType::STORAGE_BUFFER)
					.descriptor_count(frame_count),
			];
			device.create_descriptor_pool(
				&vk::DescriptorPoolCreateInfo::default()
					.max_sets(frame_count)
					.pool_sizes(&pool_sizes),
				None,
			)?
		};
		let desc_sets = unsafe {
			let layouts = vec![desc_set_layout; frame_count as usize];
			device.allocate_descriptor_sets(
				&vk::DescriptorSetAllocateInfo::default()
					.descriptor_pool(desc_pool)
					.set_layouts(&layouts),
			)?
		};

		for i in 0..frame_count as usize {
			let camera_info = [vk::DescriptorBufferInfo::default()
				.buffer(camera_buffers[i].buffer())
				.offset(0)
				.range(size_of::<CameraUniformData>() as vk::DeviceSize)];
			let light_info = [vk::DescriptorBufferInfo::default()
				.buffer(light_buffers[i].buffer())
				.offset(0)
				.range((size_of::<GPULight>() * MAX_LIGHTS) as vk::DeviceSize)];
			unsafe {
				device.update_descriptor_sets(
					&[
						vk::WriteDescriptorSet::default()
							.dst_set(desc_sets[i])
							.dst_binding(0)
							.descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
							.buffer_info(&camera_info),
						vk::WriteDescriptorSet::default()
							.dst_set(desc_sets[i])
							.dst_binding(1)
							.descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
							.buffer_info(&light_info),
					],
					&[],
				);
			}
		}

		Ok(Self {
			camera_buffers,
			light_buffers,
			shadow_casters: Mutex::new(Vec::new()),
			desc_sets,
			desc_set_layout,
			desc_pool,
			device,
		})
	}

	/// Light's own view*proj for rendering into its shadow-map layer. Same
	/// reversed-Z convention as the shadow pipeline (GREATER_OR_EQUAL, cleared to
	/// depth 0.0) -- swapped near/far achieves the reversal, matching what already
	/// worked for the single-light setup.
	fn light_view_proj(light: &DirectionalLight) -> Mat4 {
		let aspect = 1.0;
		let proj = nalgebra_glm::perspective_rh_zo(aspect, 90_f32.to_radians(), 1000.0, 0.1);
		let dir = light.direction.normalize();
		let up = if dir.y.abs() < 0.999 {
			nalgebra_glm::vec3(0.0, 1.0, 0.0)
		} else {
			nalgebra_glm::vec3(1.0, 0.0, 0.0)
		};
		let eye = light.position;
		let view = nalgebra_glm::look_at_rh(&eye, &(eye + dir), &up);
		proj * view
	}

	pub(in super::super) fn update(
		&self,
		frame_id: usize,
		extent: &vk::Extent2D,
		camera: &components::Camera,
		time: f32,
		lights: &View<DirectionalLight>,
	) {
		let aspect = extent.width as f32 / extent.height as f32;
		let mut proj = nalgebra_glm::perspective_rh_zo(aspect, 90_f32.to_radians(), 10.0, 0.1);
		let rev_z_matrix = Mat4::new(
			1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
		);
		proj *= rev_z_matrix;
		let view = camera.view_matrix();
		let view_proj = proj * view;

		let mut shadow_casters = Vec::new();
		let mut next_layer: u32 = 0;

		let gpu_lights: Vec<GPULight> = lights
			.iter()
			.take(MAX_LIGHTS)
			.map(|light| {
				let shadow_layer = if light.cast_shadow && next_layer < MAX_SHADOW_CASTERS {
					let layer = next_layer;
					next_layer += 1;
					layer as i32
				} else {
					-1
				};

				let light_view_proj = if shadow_layer >= 0 {
					let vp = Self::light_view_proj(light);
					shadow_casters.push(ShadowCasterInfo {
						layer: shadow_layer as u32,
						view_proj: vp,
					});
					vp
				} else {
					Mat4::identity()
				};

				let check_outside = if light.check_outside { 0 } else { 1 };

				GPULight {
					view_proj: light_view_proj,
					direction: nalgebra_glm::vec4(
						light.direction.x,
						light.direction.y,
						light.direction.z,
						0.0,
					),
					shadow_layer,
					check_outside,
					_pad: [0; 2],
				}
			})
			.collect();

		*self.shadow_casters.lock().unwrap() = shadow_casters;

		let camera_uniform = CameraUniformData {
			view,
			proj,
			view_proj,
			time,
			light_count: gpu_lights.len() as u32,
			_pad0: 0,
			_pad1: 0,
		};

		self.camera_buffers[frame_id].write(&camera_uniform);
		self.light_buffers[frame_id].write_slice(&gpu_lights);
	}

	pub(in super::super) fn shadow_casters(&self) -> std::sync::MutexGuard<Vec<ShadowCasterInfo>> {
		self.shadow_casters.lock().unwrap()
	}
	pub(in super::super) fn descriptor_set(&self, frame_id: usize) -> vk::DescriptorSet {
		self.desc_sets[frame_id]
	}
	pub(in super::super) fn layout(&self) -> vk::DescriptorSetLayout {
		self.desc_set_layout
	}
}

impl Drop for FrameUniforms {
	fn drop(&mut self) {
		unsafe {
			self.device.destroy_descriptor_pool(self.desc_pool, None);
			self.device
				.destroy_descriptor_set_layout(self.desc_set_layout, None);
		}
	}
}

impl StandartMaterial {
	pub fn new(
		device: Arc<Device>,
		swapchain: &Swapchain,
		gbuffers: &GBuffers,
		frame_uniforms: &FrameUniforms,
		frame_count: u32,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
		let shader = unsafe {
			let blob = super::build_shader(
				"engine/shaders/main.slang",
				&["vertMain", "fragMain", "shadow"],
			)?;
			let spv: &[u32] = bytemuck::cast_slice(blob.as_slice());
			device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(spv), None)?
		};
		let shadow_sampler = unsafe {
			device.create_sampler(
				&vk::SamplerCreateInfo::default()
					.mag_filter(vk::Filter::LINEAR)
					.min_filter(vk::Filter::LINEAR)
					.mipmap_mode(vk::SamplerMipmapMode::NEAREST)
					.address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
					.address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
					.address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
					.mip_lod_bias(0.0)
					.min_lod(0.0)
					.max_lod(0.0)
					.anisotropy_enable(false)
					.compare_enable(true)
					.compare_op(vk::CompareOp::GREATER_OR_EQUAL)
					.unnormalized_coordinates(false),
				None,
			)?
		};
		let desc_set_layout = unsafe {
			let samplers = [shadow_sampler];
			let bindings = [vk::DescriptorSetLayoutBinding::default()
				.binding(0)
				.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
				.descriptor_count(1)
				.stage_flags(vk::ShaderStageFlags::FRAGMENT)
				.immutable_samplers(&samplers)];
			device.create_descriptor_set_layout(
				&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
				None,
			)?
		};
		let desc_pool = unsafe {
			let pool_sizes = [vk::DescriptorPoolSize::default()
				.ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
				.descriptor_count(frame_count)];
			device.create_descriptor_pool(
				&vk::DescriptorPoolCreateInfo::default()
					.max_sets(frame_count)
					.pool_sizes(&pool_sizes),
				None,
			)?
		};
		let desc_sets = unsafe {
			let layouts = vec![desc_set_layout; frame_count as usize];
			device.allocate_descriptor_sets(
				&vk::DescriptorSetAllocateInfo::default()
					.descriptor_pool(desc_pool)
					.set_layouts(&layouts),
			)?
		};
		for i in 0..frame_count as usize {
			let image_info = [vk::DescriptorImageInfo::default()
				.image_view(gbuffers[i].shadow.view)
				.image_layout(vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL)];
			unsafe {
				device.update_descriptor_sets(
					&[vk::WriteDescriptorSet::default()
						.dst_set(desc_sets[i])
						.dst_binding(0)
						.descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
						.image_info(&image_info)],
					&[],
				);
			}
		}
		Ok(Self {
			shader,
			color_format: swapchain.format.format,
			depth_format: gbuffers.depth_format,
			shadow_format: gbuffers.shadow_format,
			shadow_sampler,
			frame_set_layout: frame_uniforms.layout(),
			desc_set_layout,
			desc_pool,
			desc_sets,
			device,
		})
	}

	fn create_main_pipeline(&self) -> Result<Pipeline, Box<dyn Error + Send + Sync>> {
		let push_range = vk::PushConstantRange::default()
			.stage_flags(vk::ShaderStageFlags::VERTEX)
			.offset(0)
			.size(size_of::<DrawConstants>() as u32);
		let layout = unsafe {
			self.device.create_pipeline_layout(
				&vk::PipelineLayoutCreateInfo::default()
					.set_layouts(&[self.frame_set_layout, self.desc_set_layout])
					.push_constant_ranges(&[push_range]),
				None,
			)?
		};
		let stages = [
			vk::PipelineShaderStageCreateInfo::default()
				.stage(vk::ShaderStageFlags::VERTEX)
				.module(self.shader)
				.name(c"vertMain"),
			vk::PipelineShaderStageCreateInfo::default()
				.stage(vk::ShaderStageFlags::FRAGMENT)
				.module(self.shader)
				.name(c"fragMain"),
		];

		let binding = Vertex::binding();

		let bindings = [binding];

		let attrs = Vertex::attributes();

		let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
			.vertex_binding_descriptions(&bindings)
			.vertex_attribute_descriptions(&attrs);

		let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
			.topology(vk::PrimitiveTopology::TRIANGLE_LIST);

		let viewport_state = vk::PipelineViewportStateCreateInfo::default()
			.viewport_count(1)
			.scissor_count(1);

		let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
			.polygon_mode(vk::PolygonMode::FILL)
			.cull_mode(vk::CullModeFlags::FRONT)
			.front_face(vk::FrontFace::COUNTER_CLOCKWISE)
			.line_width(1.0);

		let multisample = vk::PipelineMultisampleStateCreateInfo::default()
			.rasterization_samples(vk::SampleCountFlags::TYPE_1);

		let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
			.blend_enable(true)
			.color_write_mask(vk::ColorComponentFlags::RGBA)
			.src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
			.dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
			.color_blend_op(vk::BlendOp::ADD)
			.src_alpha_blend_factor(vk::BlendFactor::ONE)
			.dst_alpha_blend_factor(vk::BlendFactor::ZERO)
			.alpha_blend_op(vk::BlendOp::ADD);

		let attachments = [blend_attachment];

		let color_blend =
			vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);

		let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
			.depth_test_enable(true)
			.depth_write_enable(true)
			.depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);

		let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
		let dynamic_state =
			vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

		let attachment_formats = [self.color_format];

		let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
			.color_attachment_formats(&attachment_formats)
			.depth_attachment_format(vk::Format::D32_SFLOAT);

		let info = vk::GraphicsPipelineCreateInfo::default()
			.stages(&stages)
			.vertex_input_state(&vertex_input)
			.input_assembly_state(&input_assembly)
			.viewport_state(&viewport_state)
			.rasterization_state(&rasterization)
			.multisample_state(&multisample)
			.color_blend_state(&color_blend)
			.depth_stencil_state(&depth_state)
			.dynamic_state(&dynamic_state)
			.layout(layout)
			.push_next(&mut rendering_info);

		let infos = [info];
		let pipeline = unsafe {
			self.device
				.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None)
				.map_err(|(_, e)| e)?
		};
		Ok(Pipeline {
			pipeline: pipeline[0],
			layout: layout,
			device: self.device.clone(),
		})
	}

	fn create_shadow_pipeline(&self) -> Result<Pipeline, Box<dyn Error + Send + Sync>> {
		let push_range = vk::PushConstantRange::default()
			.stage_flags(vk::ShaderStageFlags::VERTEX)
			.offset(0)
			.size(size_of::<DrawConstants>() as u32);
		let layout = unsafe {
			self.device.create_pipeline_layout(
				&vk::PipelineLayoutCreateInfo::default()
					.set_layouts(&[self.frame_set_layout])
					.push_constant_ranges(&[push_range]),
				None,
			)?
		};
		let stages = [vk::PipelineShaderStageCreateInfo::default()
			.stage(vk::ShaderStageFlags::VERTEX)
			.module(self.shader)
			.name(c"shadow")];

		let binding = Vertex::binding();

		let bindings = [binding];

		let attrs = Vertex::attributes();

		let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
			.vertex_binding_descriptions(&bindings)
			.vertex_attribute_descriptions(&attrs);

		let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
			.topology(vk::PrimitiveTopology::TRIANGLE_LIST);

		let viewport_state = vk::PipelineViewportStateCreateInfo::default()
			.viewport_count(1)
			.scissor_count(1);

		let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
			.depth_bias_enable(true)
			.depth_bias_constant_factor(-1.0)
			.depth_bias_slope_factor(-1.75)
			.polygon_mode(vk::PolygonMode::FILL)
			.cull_mode(vk::CullModeFlags::FRONT)
			.front_face(vk::FrontFace::COUNTER_CLOCKWISE)
			.line_width(1.0);

		let multisample = vk::PipelineMultisampleStateCreateInfo::default()
			.rasterization_samples(vk::SampleCountFlags::TYPE_1);

		let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
			.depth_test_enable(true)
			.depth_write_enable(true)
			.depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);

		let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
		let dynamic_state =
			vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

		let mut rendering_info =
			vk::PipelineRenderingCreateInfo::default().depth_attachment_format(self.shadow_format);

		let info = vk::GraphicsPipelineCreateInfo::default()
			.stages(&stages)
			.vertex_input_state(&vertex_input)
			.input_assembly_state(&input_assembly)
			.viewport_state(&viewport_state)
			.rasterization_state(&rasterization)
			.multisample_state(&multisample)
			.depth_stencil_state(&depth_state)
			.dynamic_state(&dynamic_state)
			.layout(layout)
			.push_next(&mut rendering_info);

		let infos = [info];
		let pipeline = unsafe {
			self.device
				.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None)
				.map_err(|(_, e)| e)?
		};
		Ok(Pipeline {
			pipeline: pipeline[0],
			layout: layout,
			device: self.device.clone(),
		})
	}
}

impl Drop for StandartMaterial {
	fn drop(&mut self) {
		unsafe {
			self.device.destroy_shader_module(self.shader, None);
			self.device.destroy_sampler(self.shadow_sampler, None);
			self.device.destroy_descriptor_pool(self.desc_pool, None);
			self.device
				.destroy_descriptor_set_layout(self.desc_set_layout, None);
		}
	}
}

impl Materal for StandartMaterial {
	fn create_pipeline(&self) -> Result<Vec<(PassID, Pipeline)>, Box<dyn Error + Send + Sync>> {
		return Ok(vec![
			(PassID::Geometry, self.create_main_pipeline()?),
			(PassID::Shadow, self.create_shadow_pipeline()?),
		]);
	}

	fn bind(
		&self,
		id: u32,
		pass_id: PassID,
		cmd: &FrameCommand,
		pipeline: &Pipeline,
		ctx: &BindContext,
	) {
		unsafe {
			let constant = DrawConstants {
				model: ctx.transform.compute_matrix(),
				light_view_proj: ctx.light_view_proj.unwrap_or_else(Mat4::identity),
			};
			match pass_id {
				PassID::Geometry => {
					cmd.device.cmd_bind_descriptor_sets(
						cmd.buffer,
						vk::PipelineBindPoint::GRAPHICS,
						pipeline.layout,
						0,
						&[
							ctx.frame_uniforms.descriptor_set(ctx.frame_id),
							self.desc_sets[id as usize],
						],
						&[],
					);
				}
				PassID::Shadow => {
					cmd.device.cmd_bind_descriptor_sets(
						cmd.buffer,
						vk::PipelineBindPoint::GRAPHICS,
						pipeline.layout,
						0,
						&[ctx.frame_uniforms.descriptor_set(ctx.frame_id)],
						&[],
					);
				}
				_ => return,
			}
			cmd.device.cmd_push_constants(
				cmd.buffer,
				pipeline.layout,
				vk::ShaderStageFlags::VERTEX,
				0,
				bytes_of(&constant),
			);
		}
	}
}
