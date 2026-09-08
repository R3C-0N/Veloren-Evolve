//! Les grandeurs physiques portees par une entite.
//!
//! Ces types vivaient dans `comp::phys`, aupres de `Collider` qui, lui, ne peut
//! pas descendre (il tient un volume de voxels, donc `terrain`). Les garder
//! la-haut obligeait `comp::body` a dependre de tout `veloren-common` pour
//! nommer une masse.

use serde::{Deserialize, Serialize};
use specs::{Component, DerefFlaggedStorage, VecStorage};
use vek::Vec2;

use crate::consts::WATER_DENSITY;

// Scale
#[derive(Copy, Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scale(pub f32);

impl Component for Scale {
    type Storage = DerefFlaggedStorage<Self, VecStorage<Self>>;
}

// Mass
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Mass(pub f32);

impl Default for Mass {
    fn default() -> Mass { Mass(1.0) }
}

impl Component for Mass {
    type Storage = DerefFlaggedStorage<Self, VecStorage<Self>>;
}

/// The average density (specific mass) of an entity.
/// Units used for reference is kg/m³
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Density(pub f32);

impl Default for Density {
    fn default() -> Density { Density(WATER_DENSITY) }
}

impl Component for Density {
    type Storage = DerefFlaggedStorage<Self, VecStorage<Self>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapsulePrism {
    pub p0: Vec2<f32>,
    pub p1: Vec2<f32>,
    pub radius: f32,
    pub z_min: f32,
    pub z_max: f32,
}
