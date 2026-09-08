//! Le genre d'un outil, et le nombre de mains qu'il demande.
//!
//! `ToolKind` est un enum sans charge utile, mais il est la charge utile d'une
//! variante de `comp::body::item::Body` -- la seule dependance de structure qui
//! liait les corps aux objets. Le descendre ici la coupe, sans avoir a le
//! recopier : deux copies devraient rester alignees sur leurs ordinaux, que
//! bincode utilise pour les sauvegardes, sans que le compilateur y veille.

use serde::{Deserialize, Serialize};
use strum::EnumIter;

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
