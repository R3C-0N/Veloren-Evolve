use common_vocab::{
    consts::WATER_DENSITY,
    geometrie::{Dir, Ori},
    phys::{Density, Mass},
};
use common_vocab::outils::ToolKind;
use common_base::enum_iter;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use strum::IntoEnumIterator;
use vek::Vec3;

enum_iter! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum ItemArmorKind {
        Shoulder = 0,
        Chest = 1,
        Belt = 2,
        Hand = 3,
        Pants = 4,
        Foot = 5,
        Back = 6,
        Ring = 7,
        Neck = 8,
        Head = 9,
        Tabard = 10,
        Bag = 11,
    }
}

enum_iter! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[repr(u32)]
    pub enum Body {
        Tool(ToolKind) = 0,
        ModularComponent = 1,
        Lantern = 2,
        Glider = 3,
        Armor(ItemArmorKind) = 4,
        Utility = 5,
        Consumable = 6,
        Throwable = 7,
        Ingredient = 8,
        Coins = 9,
        CoinPouch = 10,
        Empty = 11,
        Thrown(ToolKind) = 12,
    }
}

impl From<Body> for super::Body {
    fn from(body: Body) -> Self { super::Body::Item(body) }
}

impl Body {
    pub fn to_string(self) -> &'static str {
        match self {
            Body::Tool(_) => "tool",
            Body::ModularComponent => "modular_component",
            Body::Lantern => "lantern",
            Body::Glider => "glider",
            Body::Armor(_) => "armor",
            Body::Utility => "utility",
            Body::Consumable => "consumable",
            Body::Throwable => "throwable",
            Body::Ingredient => "ingredient",
            Body::Coins => "coins",
            Body::CoinPouch => "coin_pouch",
            Body::Empty => "empty",
            Body::Thrown(_) => "thrown",
        }
    }

    pub fn density(&self) -> Density { Density(1.1 * WATER_DENSITY) }

    pub fn mass(&self) -> Mass { Mass(2.0) }

    pub fn dimensions(&self) -> Vec3<f32> { Vec3::new(0.0, 0.1, 0.0) }

    pub fn orientation(&self, rng: &mut impl RngExt) -> Ori {
        let random = rng.random_range(-1.0..1.0f32);
        let default = Ori::default();
        match self {
            Body::Tool(_) | Body::Thrown(_) => default
                .pitched_down(PI / 2.0)
                .yawed_left(PI / 2.0)
                .pitched_towards(
                    Dir::from_unnormalized(Vec3::new(
                        random,
                        rng.random_range(-1.0..1.0f32),
                        rng.random_range(-1.0..1.0f32),
                    ))
                    .unwrap_or_default(),
                ),

            Body::Armor(ItemArmorKind::Neck | ItemArmorKind::Back | ItemArmorKind::Tabard) => {
                default.yawed_left(random).pitched_down(PI / 2.0)
            },
            _ => default.yawed_left(random),
        }
    }
}
