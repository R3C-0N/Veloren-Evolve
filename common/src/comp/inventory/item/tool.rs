// Note: If you changes here "break" old character saves you can change the
// version in voxygen\src\meta.rs in order to reset save files to being empty

use crate::comp::inventory::item::DurabilityMultiplier;
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub};
use strum::EnumIter;

// La machinerie d'abilites (`AbilitySet`, `AbilityKind`, `AbilityMap`…) vivait
// ici, ce qui faisait entrer `CharacterAbility`, `Stance`, `Buffs` et `SkillSet`
// dans le module des objets. Elle est desormais dans `comp::ability`, ou elle a
// sa place ; on la reexporte pour que les chemins existants continuent de
// resoudre.
pub use crate::comp::ability::{
    AbilityContext, AbilityItem, AbilityKind, AbilityMap, AbilityMapEntry, AbilitySet,
    ContextualIndex,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd, EnumIter,
)]
pub enum ToolKind {
    // weapons
    Sword,
    Axe,
    Hammer,
    Bow,
    Staff,
    Sceptre,
    // future weapons
    Dagger,
    Shield,
    Spear,
    Blowgun,
    // tools
    Debug,
    Farming,
    Pick,
    Shovel,
    /// Music Instruments
    Instrument,
    /// Throwable item
    Throwable,
    // npcs
    /// Intended for invisible weapons (e.g. a creature using its claws or
    /// biting)
    Natural,
    /// This is an placeholder item, it is used by non-humanoid npcs to attack
    Empty,
}

impl ToolKind {
    pub fn identifier_name(&self) -> &'static str {
        match self {
            ToolKind::Sword => "sword",
            ToolKind::Axe => "axe",
            ToolKind::Hammer => "hammer",
            ToolKind::Bow => "bow",
            ToolKind::Dagger => "dagger",
            ToolKind::Staff => "staff",
            ToolKind::Spear => "spear",
            ToolKind::Blowgun => "blowgun",
            ToolKind::Sceptre => "sceptre",
            ToolKind::Shield => "shield",
            ToolKind::Natural => "natural",
            ToolKind::Debug => "debug",
            ToolKind::Farming => "farming",
            ToolKind::Pick => "pickaxe",
            ToolKind::Shovel => "shovel",
            ToolKind::Instrument => "instrument",
            ToolKind::Throwable => "throwable",
            ToolKind::Empty => "empty",
        }
    }

    pub fn gains_combat_xp(&self) -> bool {
        matches!(
            self,
            ToolKind::Sword
                | ToolKind::Axe
                | ToolKind::Hammer
                | ToolKind::Bow
                | ToolKind::Dagger
                | ToolKind::Staff
                | ToolKind::Spear
                | ToolKind::Blowgun
                | ToolKind::Sceptre
                | ToolKind::Shield
        )
    }

    pub fn can_block(&self) -> bool {
        matches!(
            self,
            ToolKind::Sword
                | ToolKind::Axe
                | ToolKind::Hammer
                | ToolKind::Shield
                | ToolKind::Dagger
        )
    }

    pub fn block_priority(&self) -> i32 {
        match self {
            ToolKind::Debug => 0,
            ToolKind::Blowgun => 1,
            ToolKind::Bow => 2,
            ToolKind::Staff => 3,
            ToolKind::Sceptre => 4,
            ToolKind::Empty => 5,
            ToolKind::Natural => 6,
            ToolKind::Throwable => 7,
            ToolKind::Instrument => 8,
            ToolKind::Farming => 9,
            ToolKind::Shovel => 10,
            ToolKind::Pick => 11,
            ToolKind::Dagger => 12,
            ToolKind::Spear => 13,
            ToolKind::Hammer => 14,
            ToolKind::Axe => 15,
            ToolKind::Sword => 16,
            ToolKind::Shield => 17,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hands {
    One,
    Two,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub equip_time_secs: f32,
    pub power: f32,
    pub effect_power: f32,
    pub speed: f32,
    pub range: f32,
    pub energy_efficiency: f32,
    pub buff_strength: f32,
}

impl Stats {
    pub fn zero() -> Stats {
        Stats {
            equip_time_secs: 0.0,
            power: 0.0,
            effect_power: 0.0,
            speed: 0.0,
            range: 0.0,
            energy_efficiency: 0.0,
            buff_strength: 0.0,
        }
    }

    pub fn one() -> Stats {
        Stats {
            equip_time_secs: 1.0,
            power: 1.0,
            effect_power: 1.0,
            speed: 1.0,
            range: 1.0,
            energy_efficiency: 1.0,
            buff_strength: 1.0,
        }
    }

    /// Calculates a diminished buff strength where the buff strength is clamped
    /// by the power, and then excess buff strength above the power is added
    /// with diminishing returns.
    // TODO: Remove this later when there are more varied high tier materials.
    // Mainly exists for now as a hack to allow some progression in strength of
    // directly applied buffs.
    pub fn diminished_buff_strength(&self) -> f32 {
        let base = self.buff_strength.clamp(0.0, self.power);
        let diminished = (self.buff_strength - base + 1.0).log(5.0);
        base + diminished
    }

    pub fn with_durability_mult(&self, dur_mult: DurabilityMultiplier) -> Self {
        let less_scaled = dur_mult.0 * 0.5 + 0.5;
        Self {
            equip_time_secs: self.equip_time_secs / less_scaled.max(0.01),
            power: self.power * dur_mult.0,
            effect_power: self.effect_power * dur_mult.0,
            speed: self.speed * less_scaled,
            range: self.range * less_scaled,
            energy_efficiency: self.energy_efficiency * less_scaled,
            buff_strength: self.buff_strength * dur_mult.0,
        }
    }
}

impl Add<Stats> for Stats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            equip_time_secs: self.equip_time_secs + other.equip_time_secs,
            power: self.power + other.power,
            effect_power: self.effect_power + other.effect_power,
            speed: self.speed + other.speed,
            range: self.range + other.range,
            energy_efficiency: self.energy_efficiency + other.energy_efficiency,
            buff_strength: self.buff_strength + other.buff_strength,
        }
    }
}

impl AddAssign<Stats> for Stats {
    fn add_assign(&mut self, other: Stats) { *self = *self + other; }
}

impl Sub<Stats> for Stats {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self {
            equip_time_secs: self.equip_time_secs - other.equip_time_secs,
            power: self.power - other.power,
            effect_power: self.effect_power - other.effect_power,
            speed: self.speed - other.speed,
            range: self.range - other.range,
            energy_efficiency: self.energy_efficiency - other.energy_efficiency,
            buff_strength: self.buff_strength - other.buff_strength,
        }
    }
}

impl Mul<Stats> for Stats {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self {
            equip_time_secs: self.equip_time_secs * other.equip_time_secs,
            power: self.power * other.power,
            effect_power: self.effect_power * other.effect_power,
            speed: self.speed * other.speed,
            range: self.range * other.range,
            energy_efficiency: self.energy_efficiency * other.energy_efficiency,
            buff_strength: self.buff_strength * other.buff_strength,
        }
    }
}

impl MulAssign<Stats> for Stats {
    fn mul_assign(&mut self, other: Stats) { *self = *self * other; }
}

impl Div<f32> for Stats {
    type Output = Self;

    fn div(self, scalar: f32) -> Self {
        Self {
            equip_time_secs: self.equip_time_secs / scalar,
            power: self.power / scalar,
            effect_power: self.effect_power / scalar,
            speed: self.speed / scalar,
            range: self.range / scalar,
            energy_efficiency: self.energy_efficiency / scalar,
            buff_strength: self.buff_strength / scalar,
        }
    }
}

impl Mul<DurabilityMultiplier> for Stats {
    type Output = Self;

    fn mul(self, value: DurabilityMultiplier) -> Self { self.with_durability_mult(value) }
}

/// A quoi un outil sert : frapper, ou creuser.
///
/// **Ce n'est pas deductible de `ToolKind`**, et c'est tout l'interet du champ.
/// `Axe` couvre les haches de guerre autant que les hachettes de bucheron, et
/// les villageois de Veloren equipent des pioches et des pelles comme armes
/// (`weapons.tool.pickaxe`, `shovel-0`). Refuser une famille entiere dans les
/// mains les desarmerait tous.
///
/// `Arme` est donc le defaut, et seuls les outils de creusement du fork
/// declarent `Creusement`. Ceux-la ne s'equipent qu'aux emplacements d'outil.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolUsage {
    #[default]
    Arme,
    Creusement,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tool {
    pub kind: ToolKind,
    pub hands: Hands,
    stats: Stats,
    #[serde(default)]
    pub usage: ToolUsage,
    // TODO: item specific abilities
}

impl Tool {
    // DO NOT USE UNLESS YOU KNOW WHAT YOU ARE DOING
    // Added for CSV import of stats
    pub fn new(kind: ToolKind, hands: Hands, stats: Stats) -> Self {
        Self {
            kind,
            hands,
            stats,
            usage: ToolUsage::Arme,
        }
    }

    pub fn empty() -> Self {
        Self {
            kind: ToolKind::Empty,
            hands: Hands::One,
            stats: Stats {
                equip_time_secs: 0.0,
                power: 1.00,
                effect_power: 1.00,
                speed: 1.00,
                range: 1.0,
                energy_efficiency: 1.0,
                buff_strength: 1.0,
            },
            usage: ToolUsage::Arme,
        }
    }

    pub fn stats(&self, durability_multiplier: DurabilityMultiplier) -> Stats {
        self.stats * durability_multiplier
    }
}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AbilitySpec {
    Tool(ToolKind),
    Custom(String),
}