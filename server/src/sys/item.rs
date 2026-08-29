use std::collections::HashMap;

use common::{
    CachedSpatialGrid, comp,
    event::{DeleteEvent, EventBus, InventoryManipEvent},
    resources::ProgramTime,
    uid::Uid,
};
use common_ecs::{Origin, Phase, System};
use specs::{Entities, Entity, Join, LendJoin, Read, ReadStorage, WriteStorage};

const MAX_ITEM_MERGE_DIST: f32 = 2.0;
const CHECKS_PER_SECOND: f64 = 10.0; // Start by checking an item 10 times every second

/// Distance a laquelle un objet pose au sol est ramasse sans rien demander.
///
/// Mesuree entre la position du joueur, qui est a ses pieds, et celle de
/// l'objet, qui est au centre du bloc d'ou il vient : un bloc casse juste
/// devant soi est deja a plus de deux metres. En dessous de 3, casser le sol a
/// ses pieds ne ramasse rien, ce qui est exactement le geste attendu.
const AUTO_PICKUP_DIST: f32 = 3.0;

#[derive(Default)]
pub struct Sys;

impl<'a> System<'a> for Sys {
    type SystemData = (
        Entities<'a>,
        WriteStorage<'a, comp::PickupItem>,
        ReadStorage<'a, comp::Pos>,
        ReadStorage<'a, comp::LootOwner>,
        Read<'a, CachedSpatialGrid>,
        Read<'a, ProgramTime>,
        Read<'a, EventBus<DeleteEvent>>,
        ReadStorage<'a, comp::Player>,
        ReadStorage<'a, comp::Inventory>,
        ReadStorage<'a, comp::Health>,
        ReadStorage<'a, Uid>,
        Read<'a, EventBus<InventoryManipEvent>>,
    );

    const NAME: &'static str = "item";
    const ORIGIN: Origin = Origin::Server;
    const PHASE: Phase = Phase::Create;

    fn run(
        _job: &mut common_ecs::Job<Self>,
        (
            entities,
            mut items,
            positions,
            loot_owners,
            spatial_grid,
            program_time,
            delete_bus,
            players,
            inventories,
            healths,
            uids,
            inventory_manip_bus,
        ): Self::SystemData,
    ) {
        // --- Ramassage automatique ---------------------------------------
        // On n'insere rien dans l'inventaire ici : on emet l'evenement que
        // produit deja la touche de ramassage. Toutes les verifications
        // existantes s'appliquent donc telles quelles — appartenance du butin,
        // inventaire plein, suppression de l'entite, notification du client —
        // et il n'y a pas deux chemins de ramassage a maintenir.
        {
            let mut manip_emitter = inventory_manip_bus.emitter();
            for (player_entity, _, player_pos, inventory, health) in (
                &entities,
                &players,
                &positions,
                &inventories,
                healths.maybe(),
            )
                .join()
            {
                // Un mort ne ramasse pas.
                if health.is_some_and(|h| h.is_dead) {
                    continue;
                }
                let Some(player_uid) = uids.get(player_entity).copied() else {
                    continue;
                };

                for item_entity in spatial_grid
                    .0
                    .in_circle_aabr(player_pos.0.xy(), AUTO_PICKUP_DIST)
                {
                    let Some((item_entity, item, item_pos, loot_owner)) =
                        (&entities, &items, &positions, loot_owners.maybe())
                            .lend_join()
                            .get(item_entity, &entities)
                    else {
                        continue;
                    };

                    // La grille spatiale ne filtre qu'en XY : il faut la
                    // distance reelle, sinon on ramasse a travers un plafond.
                    if item_pos.0.distance_squared(player_pos.0) >= AUTO_PICKUP_DIST.powi(2) {
                        continue;
                    }

                    // Butin reserve a quelqu'un d'autre : on n'y touche pas.
                    // Le butin de groupe (`uid()` vaut None) reste au ramassage
                    // manuel, ou la verification complete a lieu.
                    if let Some(owner) = loot_owner
                        && !owner.expired()
                        && owner.uid() != Some(player_uid)
                    {
                        continue;
                    }

                    // Sans place, ne rien emettre : le ramassage echouerait et
                    // notifierait le client a chaque tick.
                    if inventory.free_slots() == 0 && !inventory.can_stack(item.item()) {
                        continue;
                    }

                    let Some(item_uid) = uids.get(item_entity).copied() else {
                        continue;
                    };
                    manip_emitter.emit(InventoryManipEvent(
                        player_entity,
                        comp::InventoryManip::Pickup(item_uid),
                    ));
                }
            }
        }

        // Contains items that have been checked for merge, or that were merged into
        // another one
        let mut merged = HashMap::new();
        // Contains merges that will be performed (from, into)
        let mut merges = Vec::new();
        // Delete events are emitted when this is dropped
        let mut delete_emitter = delete_bus.emitter();

        for (entity, item, pos, loot_owner) in
            (&entities, &items, &positions, loot_owners.maybe()).join()
        {
            // Do not process items that are already being merged
            if merged.contains_key(&entity) {
                continue;
            }

            // For items that merge, exponentially back off the frequency of the merge check
            if !item.should_merge || program_time.0 < item.next_merge_check().0 {
                continue;
            }

            // We do not want to allow merging this item if it isn't already being
            // merged into another
            merged.insert(entity, true);

            for (source_entity, _) in get_nearby_mergeable_items(
                item,
                pos,
                loot_owner,
                (&entities, &items, &positions, &loot_owners, &spatial_grid),
            ) {
                // Prevent merging an item multiple times, we cannot
                // do this in the above filter since we mutate `merged` below
                if merged.contains_key(&source_entity) {
                    continue;
                }

                // Do not merge items multiple times
                merged.insert(source_entity, false);
                // Defer the merge
                merges.push((source_entity, entity));
            }
        }

        for (source, target) in merges {
            let source_item = items
                .remove(source)
                .expect("We know this entity must have an item.");
            let mut target_item = items
                .get_mut(target)
                .expect("We know this entity must have an item.");

            if let Err(item) = target_item.try_merge(source_item) {
                // We re-insert the item, should be unreachable since we already checked whether
                // the items were mergeable in the above loop
                items
                    .insert(source, item)
                    .expect("PickupItem was removed from this entity earlier");
            } else {
                // If the merging was successfull, we remove the old item entity from the ECS
                delete_emitter.emit(DeleteEvent(source));
            }
        }

        for updated in merged
            .into_iter()
            .filter_map(|(entity, is_merge_parent)| is_merge_parent.then_some(entity))
        {
            if let Some(mut item) = items.get_mut(updated) {
                item.next_merge_check_mut().0 +=
                    (program_time.0 - item.created().0).max(1.0 / CHECKS_PER_SECOND);
            }
        }
    }
}

pub fn get_nearby_mergeable_items<'a>(
    item: &'a comp::PickupItem,
    pos: &'a comp::Pos,
    loot_owner: Option<&'a comp::LootOwner>,
    (entities, items, positions, loot_owners, spatial_grid): (
        &'a Entities<'a>,
        // We do not actually need write access here, but currently all callers of this function
        // have a WriteStorage<Item> in scope which we cannot *downcast* into a ReadStorage
        &'a WriteStorage<'a, comp::PickupItem>,
        &'a ReadStorage<'a, comp::Pos>,
        &'a ReadStorage<'a, comp::LootOwner>,
        &'a CachedSpatialGrid,
    ),
) -> impl Iterator<Item = (Entity, f32)> + 'a {
    // Get nearby items
    spatial_grid
        .0
        .in_circle_aabr(pos.0.xy(), MAX_ITEM_MERGE_DIST)
        // Filter out any unrelated entities
        .flat_map(move |entity| {
            (entities, items, positions, loot_owners.maybe())
                .lend_join()
                .get(entity, entities)
                .and_then(|(entity, item, other_position, loot_owner)| {
                    let distance_sqrd = other_position.0.distance_squared(pos.0);
                    if distance_sqrd < MAX_ITEM_MERGE_DIST.powi(2) {
                        Some((entity, item, distance_sqrd, loot_owner))
                    } else {
                        None
                    }
                })
        })
        // Filter by "mergeability"
        .filter_map(move |(entity, other_item, distance, other_loot_owner)| {
            (other_loot_owner.map(|owner| owner.owner()) == loot_owner.map(|owner| owner.owner())
                && item.can_merge(other_item)).then_some((entity, distance))
        })
}
