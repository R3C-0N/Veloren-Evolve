//! Les gabarits de voxels des aeronefs : leur geometrie de collision et les
//! ressources qui la decrivent.
//!
//! Ce module vivait dans `comp::body::ship`, ou son propre commentaire disait
//! qu'il n'y etait que pour eviter un refactoring -- « Duplicate of some of the
//! things defined in `voxygen::scene::figure::load` to avoid having to refactor
//! all of that to `common` ». Sa presence y faisait dependre tout `body` de
//! `terrain` et de `figure`. Il est ici, aupres de `figure`, dont il se sert.
//!
//! `comp::body::ship::figuredata` reste un alias vers ce module, si bien
//! qu'aucun des onze sites qui le nomment ne change.

use crate::{
    assets::{Asset, AssetCache, AssetExt, AssetHandle, BoxedError, DotVox, Ron, SharedString},
    figure::TerrainSegment,
    terrain::{Block, BlockKind, SpriteKind, StructureSprite, structure::load_base_structure},
};
use hashbrown::HashMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use vek::{Rgb, Vec3};

#[derive(Deserialize)]
pub struct VoxSimple(pub String);

#[derive(Deserialize)]
pub struct ShipCentralSpec(pub HashMap<crate::comp::body::ship::Body, SidedShipCentralVoxSpec>);

#[derive(Deserialize)]
pub enum DeBlock {
    Block(BlockKind),
    Air(StructureSprite),
    Water(StructureSprite),
}

impl DeBlock {
    fn to_block(&self, color: Rgb<u8>) -> Block {
        match *self {
            DeBlock::Block(block) => Block::new(block, color),
            DeBlock::Air(sprite) => sprite
                .apply_to_block(Block::air(SpriteKind::Empty))
                .unwrap_or_else(|b| b),
            DeBlock::Water(sprite) => sprite
                .apply_to_block(Block::water(SpriteKind::Empty))
                .unwrap_or_else(|b| b),
        }
    }
}

#[derive(Deserialize)]
pub struct SidedShipCentralVoxSpec {
    pub bone0: ShipCentralSubSpec,
    pub bone1: ShipCentralSubSpec,
    pub bone2: ShipCentralSubSpec,
    pub bone3: ShipCentralSubSpec,

    // TODO: Use StructureBlock here instead. Which would require passing `IndexRef` and
    // `Calendar` when loading the voxel colliders, which wouldn't work while it's stored in a
    // static.
    #[serde(default)]
    pub custom_indices: HashMap<u8, DeBlock>,
}

#[derive(Deserialize)]
pub struct ShipCentralSubSpec {
    pub offset: [f32; 3],
    pub central: VoxSimple,
    #[serde(default)]
    pub model_index: u32,
}

/// manual instead of through `make_vox_spec!` so that it can be in `common`
#[derive(Clone)]
pub struct ShipSpec {
    pub central: AssetHandle<Ron<ShipCentralSpec>>,
    pub colliders: HashMap<String, VoxelCollider>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoxelCollider {
    pub(super) dyna: TerrainSegment,
    pub translation: Vec3<f32>,
    /// This value should be incremented every time the volume is mutated
    /// and can be used to keep track of volume changes.
    pub mut_count: usize,
}

impl VoxelCollider {
    pub fn from_fn<F: FnMut(Vec3<i32>) -> Block>(sz: Vec3<u32>, f: F) -> Self {
        Self {
            dyna: TerrainSegment::from_fn(sz, (), f),
            translation: -sz.map(|e| e as f32) / 2.0,
            mut_count: 0,
        }
    }

    pub fn volume(&self) -> &TerrainSegment { &self.dyna }
}

impl Asset for ShipSpec {
    fn load(cache: &AssetCache, _: &SharedString) -> Result<Self, BoxedError> {
        let manifest: AssetHandle<Ron<ShipCentralSpec>> =
            AssetExt::load("common.manifests.ship_manifest")?;
        let mut colliders = HashMap::new();
        for (_, spec) in (manifest.read().0).0.iter() {
            for (index, bone) in [&spec.bone0, &spec.bone1, &spec.bone2, &spec.bone3]
                .iter()
                .enumerate()
            {
                // TODO: Currently both client and server load models and manifests from
                // "common.voxel.". In order to support CSG procedural airships, we probably
                // need to load them in the server and sync them as an ECS resource.
                let vox = cache.load::<DotVox>(&["common.voxel.", &bone.central.0].concat())?;

                let base_structure = load_base_structure(&vox.read().0, |col| col);
                let dyna = base_structure.vol.map_into(|cell| {
                    if let Some(i) = cell {
                        let color = base_structure.palette[u8::from(i) as usize];
                        if let Some(block) = spec.custom_indices.get(&i.get())
                            && index == 0
                        {
                            block.to_block(color)
                        } else {
                            Block::new(BlockKind::Misc, color)
                        }
                    } else {
                        Block::empty()
                    }
                });
                let collider = VoxelCollider {
                    dyna,
                    translation: Vec3::from(bone.offset),
                    mut_count: 0,
                };
                colliders.insert(bone.central.0.clone(), collider);
            }
        }
        Ok(ShipSpec {
            central: manifest,
            colliders,
        })
    }
}

lazy_static! {
    // TODO: Load this from the ECS as a resource, and maybe make it more general than ships
    // (although figuring out how to keep the figure bones in sync with the terrain offsets seems
    // like a hard problem if they're not the same manifest)
    pub static ref VOXEL_COLLIDER_MANIFEST: AssetHandle<ShipSpec> = AssetExt::load_expect("common.manifests.ship_manifest");
}

#[test]
fn test_ship_manifest_entries() {
    for body in crate::comp::body::ship::ALL_BODIES {
        if let Some(entry) = body.manifest_entry() {
            assert!(
                VOXEL_COLLIDER_MANIFEST
                    .read()
                    .colliders
                    .get(entry)
                    .is_some()
            );
        }
    }
}
