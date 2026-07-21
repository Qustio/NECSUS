use std::error::Error;

use nalgebra::{Vector3, vector};
use rapier3d::{
	dynamics::{
		CCDSolver, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet, RigidBodyBuilder, RigidBodySet,
	}, geometry::{ColliderSet, DefaultBroadPhase, NarrowPhase}, math::Vec3, pipeline::PhysicsPipeline,
};
use shipyard::{
	AllStoragesViewMut, Component, IntoIter, Label, Unique, UniqueView, UniqueViewMut, View,
	ViewMut, scheduler::IntoWorkloadTrySystem,
};

use crate::{
	State,
	modules::{Module, System, components::Transform, core::Time},
};

pub struct PhysicsModule;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Label)]
pub struct PhysicsStep;

// export collider builder
pub use rapier3d::geometry::ColliderBuilder;

impl Module for PhysicsModule {
	fn build(engine: &mut crate::Engine) -> Result<(), Box<dyn std::error::Error>> {
		let pos = engine
			.states
			.iter()
			.position(|s| s.dyn_eq(&State::PreUpdate))
			.unwrap();
		engine.states.insert(pos, Box::new(PhysicsStep));

		engine.systems.push(System::new(
			Box::new(State::Startup),
			setup_physics.into_workload_try_system()?,
		));
		engine.systems.push(System::new(
			Box::new(PhysicsStep),
			step.into_workload_try_system()?,
		));
		Ok(())
	}
}

#[derive(Default, Unique)]
pub struct RapierData {
	rigid_body_set: RigidBodySet,
	collider_set: ColliderSet,
	gravity: Vec3,
	integration_parameters: IntegrationParameters,
	physics_pipeline: PhysicsPipeline,
	island_manager: IslandManager,
	broad_phase: DefaultBroadPhase,
	narrow_phase: NarrowPhase,
	impulse_joint_set: ImpulseJointSet,
	multibody_joint_set: MultibodyJointSet,
	ccd_solver: CCDSolver,
	accumulator: f32,
}

impl RapierData {
	fn new() -> Self {
		Self {
			gravity: Vec3::new(0.0, -9.81, 0.0),
			..Default::default()
		}
	}
	fn step(&mut self) {
		self.physics_pipeline.step(
			self.gravity,
			&self.integration_parameters,
			&mut self.island_manager,
			&mut self.broad_phase,
			&mut self.narrow_phase,
			&mut self.rigid_body_set,
			&mut self.collider_set,
			&mut self.impulse_joint_set,
			&mut self.multibody_joint_set,
			&mut self.ccd_solver,
			&(),
			&(),
		);
	}

	pub fn insert_fixed(
		&mut self,
		transform: Transform,
		collider: rapier3d::geometry::Collider,
	) -> (
		rapier3d::dynamics::RigidBodyHandle,
		rapier3d::geometry::ColliderHandle,
	) {
		let body = RigidBodyBuilder::fixed()
			.translation(transform.translation.into())
			.rotation(transform.rotation.scaled_axis().into())
			.build();
		self.insert_rigid_body(body, collider)
	}

	pub fn insert_dynamic(
		&mut self,
		transform: Transform,
		collider: rapier3d::geometry::Collider,
	) -> (
		rapier3d::dynamics::RigidBodyHandle,
		rapier3d::geometry::ColliderHandle,
	) {
		let body = RigidBodyBuilder::dynamic()
			.translation(transform.translation.into())
			.rotation(transform.rotation.scaled_axis().into())
			.angvel(Vec3::new(0.3, 5.0, 0.0))
			.build();
		self.insert_rigid_body(body, collider)
	}

	pub fn insert_rigid_body(
		&mut self,
		body: rapier3d::dynamics::RigidBody,
		collider: rapier3d::geometry::Collider,
	) -> (
		rapier3d::dynamics::RigidBodyHandle,
		rapier3d::geometry::ColliderHandle,
	) {
		let rb_handle = self.rigid_body_set.insert(body);
		let co_handle =
			self.collider_set
				.insert_with_parent(collider, rb_handle, &mut self.rigid_body_set);
		(rb_handle, co_handle)
	}
}

fn setup_physics(world: AllStoragesViewMut) -> Result<(), Box<dyn Error + Send + Sync>> {
	let rapier_data = RapierData::new();
	world.add_unique(rapier_data);
	Ok(())
}

#[derive(Component)]
pub struct RigidBody(pub rapier3d::dynamics::RigidBodyHandle);
#[derive(Component)]
pub struct Collider(pub rapier3d::geometry::ColliderHandle);

fn step(
	time: UniqueView<Time>,
	mut rapier_data: UniqueViewMut<RapierData>,
	rigid_bodies: View<RigidBody>,
	mut transforms: ViewMut<Transform>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
	rapier_data.accumulator += time.delta.as_secs_f32();
	let mut updated = false;
	while rapier_data.accumulator >= rapier_data.integration_parameters.dt {
		rapier_data.step();
		updated = true;
		rapier_data.accumulator -= rapier_data.integration_parameters.dt
	}
	if updated {
		for (body, transform) in (&rigid_bodies, &mut transforms).iter() {
			let pose = *rapier_data.rigid_body_set[body.0].position();
			let iso: nalgebra::Isometry3<f32> = pose.into();
			transform.translation = iso.translation.vector;
			transform.rotation = iso.rotation;
		}
	}
	Ok(())
}
