//! La construction : poser un bloc en consommant un objet de l'inventaire.
//!
//! Le pendant de la casse, qui vit dans `sys::msg::in_game` et fait tomber un
//! objet. Ici on fait le chemin inverse, et **le client ne choisit rien** : il
//! n'envoie qu'un emplacement d'inventaire, le serveur lit l'objet qui s'y
//! trouve et en deduit le bloc par `terrain::block::block_from_item`.
//!
//! Pourquoi un evenement plutot qu'un traitement dans le systeme de messages :
//! lire l'objet et le retirer doivent se faire d'un seul tenant. Le systeme de
//! messages tourne en `par_join` et l'inventaire n'y est pas accessible en
//! ecriture ; deux poses arrivees dans le meme tick avec une seule pierre en
//! poche poseraient deux blocs pour un objet consomme.

use common::{
    comp::{self, InventoryUpdateEvent},
    event::PlaceBlockEvent,
    terrain::{TerrainGrid, block::block_from_item},
    vol::ReadVol,
};
use common_state::BlockChange;
use specs::{DispatcherBuilder, ReadExpect, WriteExpect, WriteStorage};

use super::{ServerEvent, event_dispatch};

pub(super) fn register_event_systems(builder: &mut DispatcherBuilder) {
    event_dispatch::<PlaceBlockEvent>(builder, &[]);
}

impl ServerEvent for PlaceBlockEvent {
    type SystemData<'a> = (
        WriteExpect<'a, BlockChange>,
        ReadExpect<'a, TerrainGrid>,
        ReadExpect<'a, comp::item::MaterialStatManifest>,
        ReadExpect<'a, comp::item::tool::AbilityMap>,
        WriteStorage<'a, comp::Inventory>,
        WriteStorage<'a, comp::InventoryUpdateBuffer>,
        crate::sys::msg::in_game::TerrainPersistenceData<'a>,
    );

    fn handle(
        events: impl ExactSizeIterator<Item = Self>,
        (
            mut block_change,
            terrain,
            msm,
            ability_map,
            mut inventories,
            mut inventory_update_buffers,
            mut _terrain_persistence,
        ): Self::SystemData<'_>,
    ) {
        for ev in events {
            let Some(mut inventory) = inventories.get_mut(ev.entity) else {
                continue;
            };

            // L'objet decide du bloc. Ce qui n'est pas un objet-bloc ne pose
            // rien, quoi qu'en dise le client.
            let Some(block) = inventory
                .get(ev.slot)
                .and_then(|item| item.item_definition_id().itemdef_id().map(String::from))
                .as_deref()
                .and_then(block_from_item)
            else {
                continue;
            };

            // On ne pose que dans du vide. Sans ce controle, poser de la terre
            // sur de la pierre effacerait la pierre sans rien lacher : de la
            // matiere perdue, et un moyen de detruire sans outil.
            if terrain
                .get(ev.pos)
                .is_ok_and(|cible| cible.kind().is_filled())
            {
                continue;
            }

            // Le bloc d'abord, l'objet ensuite : si la position a deja ete
            // modifiee ce tick, rien n'est consomme.
            if block_change.try_set(ev.pos, block).is_none() {
                continue;
            }

            inventory.take(ev.slot, &ability_map, &msm);

            #[cfg(feature = "persistent_world")]
            if let Some(terrain_persistence) = _terrain_persistence.as_mut() {
                terrain_persistence.set_block(ev.pos, block);
            }

            if let Some(buf) = inventory_update_buffers.get_mut(ev.entity) {
                buf.push(InventoryUpdateEvent::Used);
            }
        }
    }
}
