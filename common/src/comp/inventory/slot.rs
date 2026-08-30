use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, convert::TryFrom};

use crate::comp::inventory::{
    item::{ItemKind, armor, armor::ArmorKind, tool},
    loadout::LoadoutSlotId,
};

#[derive(Debug, PartialEq, Eq)]
pub enum SlotError {
    InventoryFull,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Slot {
    Inventory(InvSlotId),
    Equip(EquipSlot),
    Overflow(usize),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvSlotId {
    // The index of the loadout item that provides this inventory slot. 0 represents
    // built-in inventory slots
    loadout_idx: u16,
    // The index of the slot within its container
    slot_idx: u16,
}

impl InvSlotId {
    pub const fn new(loadout_idx: u16, slot_idx: u16) -> Self {
        Self {
            loadout_idx,
            slot_idx,
        }
    }

    pub fn idx(&self) -> u32 { (u32::from(self.loadout_idx) << 16) | u32::from(self.slot_idx) }

    pub fn loadout_idx(&self) -> usize { usize::from(self.loadout_idx) }

    pub fn slot_idx(&self) -> usize { usize::from(self.slot_idx) }
}

impl From<LoadoutSlotId> for InvSlotId {
    fn from(loadout_slot_id: LoadoutSlotId) -> Self {
        Self {
            loadout_idx: u16::try_from(loadout_slot_id.loadout_idx + 1).unwrap(),
            slot_idx: u16::try_from(loadout_slot_id.slot_idx).unwrap(),
        }
    }
}

impl PartialOrd for InvSlotId {
    fn partial_cmp(&self, other: &InvSlotId) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl Ord for InvSlotId {
    fn cmp(&self, other: &InvSlotId) -> Ordering { self.idx().cmp(&other.idx()) }
}

pub(super) enum SlotId {
    Inventory(usize),
    Loadout(LoadoutSlotId),
}

impl From<InvSlotId> for SlotId {
    fn from(inv_slot_id: InvSlotId) -> Self {
        match inv_slot_id.loadout_idx {
            0 => SlotId::Inventory(inv_slot_id.slot_idx()),
            _ => SlotId::Loadout(LoadoutSlotId {
                loadout_idx: inv_slot_id.loadout_idx() - 1,
                slot_idx: inv_slot_id.slot_idx(),
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    Armor(ArmorSlot),
    ActiveMainhand,
    ActiveOffhand,
    InactiveMainhand,
    InactiveOffhand,
    /// Les outils de creusement, hors des mains — un par famille.
    ///
    /// C'est ce qui fait qu'on ne degaine plus pour miner : le bloc designe sa
    /// famille, le serveur lit l'emplacement, et ce qu'on tient en main n'a
    /// aucun effet sur le creusement.
    Outil(FamilleOutil),
    Lantern,
    Glider,
}

/// Les trois familles d'outil de creusement, et leurs emplacements.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum FamilleOutil {
    Pioche,
    Hache,
    Pelle,
}

impl FamilleOutil {
    /// La famille qui correspond a ce type d'outil, s'il en est un.
    pub fn depuis_tool_kind(kind: tool::ToolKind) -> Option<Self> {
        Some(match kind {
            tool::ToolKind::Pick => Self::Pioche,
            tool::ToolKind::Axe => Self::Hache,
            tool::ToolKind::Shovel => Self::Pelle,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ArmorSlot {
    Head,
    Neck,
    Shoulders,
    Chest,
    Hands,
    Ring1,
    Ring2,
    Back,
    Belt,
    Legs,
    Feet,
    Tabard,
    Bag1,
    Bag2,
    Bag3,
    Bag4,
}

impl EquipSlot {
    pub fn can_hold(self, item_kind: &ItemKind) -> bool {
        match (self, item_kind) {
            (Self::Armor(slot), ItemKind::Armor(armor::Armor { kind, .. })) => slot.can_hold(kind),
            // Les mains refusent les outils de creusement — et le refus passe
            // par `usage`, jamais par la famille : `ToolKind::Axe` couvre les
            // haches de guerre, et les villageois portent pioches et pelles
            // comme armes.
            (Self::ActiveMainhand, ItemKind::Tool(tool)) => tool.usage == tool::ToolUsage::Arme,
            (Self::ActiveOffhand, ItemKind::Tool(tool)) => {
                tool.usage == tool::ToolUsage::Arme && matches!(tool.hands, tool::Hands::One)
            },
            (Self::InactiveMainhand, ItemKind::Tool(tool)) => tool.usage == tool::ToolUsage::Arme,
            (Self::InactiveOffhand, ItemKind::Tool(tool)) => {
                tool.usage == tool::ToolUsage::Arme && matches!(tool.hands, tool::Hands::One)
            },
            // L'emplacement d'outil accepte sur la seule famille : une pioche
            // de Veloren, qui reste une arme, y rentre aussi.
            (Self::Outil(famille), ItemKind::Tool(tool)) => {
                FamilleOutil::depuis_tool_kind(tool.kind) == Some(famille)
            },
            (Self::Lantern, ItemKind::Lantern(_)) => true,
            (Self::Glider, ItemKind::Glider) => true,
            _ => false,
        }
    }
}

impl ArmorSlot {
    fn can_hold(self, armor: &ArmorKind) -> bool {
        matches!(
            (self, armor),
            (Self::Head, ArmorKind::Head)
                | (Self::Neck, ArmorKind::Neck)
                | (Self::Shoulders, ArmorKind::Shoulder)
                | (Self::Chest, ArmorKind::Chest)
                | (Self::Hands, ArmorKind::Hand)
                | (Self::Ring1, ArmorKind::Ring)
                | (Self::Ring2, ArmorKind::Ring)
                | (Self::Back, ArmorKind::Back)
                | (Self::Back, ArmorKind::Backpack)
                | (Self::Belt, ArmorKind::Belt)
                | (Self::Legs, ArmorKind::Pants)
                | (Self::Feet, ArmorKind::Foot)
                | (Self::Tabard, ArmorKind::Tabard)
                | (Self::Bag1, ArmorKind::Bag)
                | (Self::Bag2, ArmorKind::Bag)
                | (Self::Bag3, ArmorKind::Bag)
                | (Self::Bag4, ArmorKind::Bag)
        )
    }
}
