use common::comp::{
    self,
    inventory::item::{Item, item_key::ItemKey},
};
use serde::{Deserialize, Serialize};

use super::HudInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Slot {
    #[default]
    One = 0,
    Two = 1,
    Three = 2,
    Four = 3,
    Five = 4,
    Six = 5,
    Seven = 6,
    Eight = 7,
    Nine = 8,
    Ten = 9,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SlotContents {
    Inventory(u64, ItemKey),
    Ability(usize),
}

/// Laquelle des deux barres repond.
///
/// Elle n'est pas un etat qu'on garde : elle se deduit de `ModeDeJeu`, que le
/// serveur ecrit et que le HUD relit a chaque image. Une seule source de
/// verite, donc la barre suit toute seule le coup recu qui met en combat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Barre {
    #[default]
    Aventure = 0,
    Combat = 1,
}

/// La barre d'objets, en deux exemplaires.
///
/// L'aventure ne porte que de la matiere, le combat que des potions et des
/// capacites. C'est ce qui permet aux deux clics d'appartenir entierement a
/// l'un ou a l'autre : plus rien a filtrer case par case.
///
/// Chaque barre garde **sa** selection, pour qu'un aller-retour rende la case
/// qu'on avait.
#[derive(Clone, Default)]
pub struct State {
    barres: [[Option<SlotContents>; 10]; 2],
    inputs: [bool; 10],
    selections: [Slot; 2],
    barre: Barre,
}

impl State {
    pub fn new(aventure: [Option<SlotContents>; 10], combat: [Option<SlotContents>; 10]) -> Self {
        Self {
            barres: [aventure, combat],
            inputs: [false; 10],
            selections: [Slot::default(); 2],
            barre: Barre::default(),
        }
    }

    pub fn barre(&self) -> Barre { self.barre }

    pub fn definir_barre(&mut self, barre: Barre) { self.barre = barre; }

    pub fn slots(&self) -> &[Option<SlotContents>; 10] { self.slots_de(self.barre) }

    pub fn slots_de(&self, barre: Barre) -> &[Option<SlotContents>; 10] {
        &self.barres[barre as usize]
    }

    pub fn selection(&self) -> Slot { self.selections[self.barre as usize] }

    pub fn selection_de(&self, barre: Barre) -> Slot { self.selections[barre as usize] }

    pub fn definir_selection(&mut self, slot: Slot) {
        self.selections[self.barre as usize] = slot;
    }

    pub fn selection_suivante(&mut self) { self.selections[self.barre as usize].next_slot(); }

    pub fn selection_precedente(&mut self) {
        self.selections[self.barre as usize].previous_slot();
    }

    /// Returns true if the button was just pressed
    pub fn process_input(&mut self, slot: Slot, state: bool) -> bool {
        let slot = slot as usize;
        let just_pressed = !self.inputs[slot] && state;
        self.inputs[slot] = state;
        just_pressed
    }

    pub fn get(&self, slot: Slot) -> Option<SlotContents> { self.get_de(self.barre, slot) }

    pub fn get_de(&self, barre: Barre, slot: Slot) -> Option<SlotContents> {
        self.barres[barre as usize][slot as usize].clone()
    }

    pub fn swap(&mut self, a: Slot, b: Slot) {
        self.barres[self.barre as usize].swap(a as usize, b as usize);
    }

    pub fn clear_slot(&mut self, slot: Slot) {
        self.barres[self.barre as usize][slot as usize] = None;
    }

    pub fn add_inventory_link(&mut self, slot: Slot, item: &Item) {
        self.add_inventory_link_de(self.barre, slot, item);
    }

    pub fn add_inventory_link_de(&mut self, barre: Barre, slot: Slot, item: &Item) {
        self.barres[barre as usize][slot as usize] = Some(SlotContents::Inventory(
            item.item_hash(),
            ItemKey::from(item),
        ));
    }

    // TODO: remove pending UI
    // Adds ability slots if missing and should be present
    // Removes ability slots if not there and shouldn't be present
    //
    // **La barre de combat seule.** Les capacites ne peuvent plus ecraser une
    // case de matiere : elles n'ont acces qu'a l'autre barre.
    pub fn maintain_abilities(&mut self, client: &client::Client, info: &HudInfo) {
        use specs::WorldExt;
        let combat = &mut self.barres[Barre::Combat as usize];
        if let Some(active_abilities) = client
            .state()
            .ecs()
            .read_storage::<comp::ActiveAbilities>()
            .get(info.viewpoint_entity)
        {
            use common::comp::ability::AuxiliaryAbility;
            for ((i, ability), hotbar_slot) in active_abilities
                .auxiliary_set(
                    client.inventories().get(info.viewpoint_entity),
                    client
                        .state()
                        .read_storage::<comp::SkillSet>()
                        .get(info.viewpoint_entity),
                )
                .iter()
                .enumerate()
                .zip(combat.iter_mut())
            {
                if matches!(ability, AuxiliaryAbility::Empty) {
                    if matches!(hotbar_slot, Some(SlotContents::Ability(_))) {
                        // If ability is empty but hotbar shows an ability, clear it
                        *hotbar_slot = None;
                    }
                } else {
                    // If an ability is not empty show it on the hotbar
                    *hotbar_slot = Some(SlotContents::Ability(i));
                }
            }
        } else {
            combat
                .iter_mut()
                .filter(|slot| matches!(slot, Some(SlotContents::Ability(_))))
                .for_each(|slot| *slot = None)
        }
    }
}

impl Slot {
    const SLOTS: [Slot; 10] = [
        Slot::One,
        Slot::Two,
        Slot::Three,
        Slot::Four,
        Slot::Five,
        Slot::Six,
        Slot::Seven,
        Slot::Eight,
        Slot::Nine,
        Slot::Ten,
    ];

    pub fn iter() -> impl Iterator<Item = Slot> { Self::SLOTS.into_iter() }

    pub fn next_slot(&mut self) {
        let current_slot = *self as usize;
        let next_slot = (current_slot + 1) % 10;
        *self = Self::SLOTS[next_slot];
    }

    pub fn previous_slot(&mut self) {
        let current_slot = *self as usize;
        let previous_slot = (current_slot + 10 - 1) % 10;
        *self = Self::SLOTS[previous_slot];
    }
}
