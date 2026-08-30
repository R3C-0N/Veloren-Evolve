//! La construction : creuser un bloc jusqu'a ce qu'il cede, et en poser un en
//! consommant un objet de l'inventaire.
//!
//! Les deux gestes vivent ici parce qu'ils partagent la meme contrainte :
//! **lire l'inventaire et le modifier doivent se faire d'un seul tenant.** Le
//! systeme de messages tourne en `par_join` et n'y a pas acces ; deux poses
//! arrivees dans le meme tick avec une seule pierre en poche poseraient deux
//! blocs pour un objet consomme.
//!
//! Poser : **le client ne choisit rien**. Il n'envoie qu'un emplacement
//! d'inventaire, le serveur lit l'objet qui s'y trouve et en deduit le bloc par
//! `terrain::block::block_from_item`.
//!
//! Creuser : le client ne fait que redire « je creuse ici » a chaque tick. Le
//! serveur accumule, decide seul quand le bloc cede, et decide seul si quelque
//! chose tombe — c'est la que le grade de l'outil verrouille le butin.
//!
//! **L'outil ne vient pas de la main** mais de son emplacement d'equipement :
//! le bloc designe sa famille, on lit l'emplacement. Et **on ne creuse pas en
//! combat** — le clic gauche y frappe, on barricade au lieu de miner.

use common::{
    comp::{self, InventoryUpdateEvent, Ori, PickupItem, Pos, Vel, inventory::item::ItemDesc},
    consts::DELAI_ABANDON_CREUSEMENT,
    creusement::{lache_son_objet, outil_pour, temps_de_casse},
    event::{CreateItemDropEvent, CreuseBlocEvent, EventBus, PlaceBlockEvent},
    outcome::Outcome,
    resources::{ProgramTime, Time},
    terrain::{TerrainGrid, block::block_from_item},
    util::Dir,
    vol::ReadVol,
};
use common_state::BlockChange;
use hashbrown::HashMap;
use specs::{
    DispatcherBuilder, Entity as EcsEntity, Read, ReadExpect, ReadStorage, WriteExpect,
    WriteStorage,
};
use vek::*;

use super::{ServerEvent, event_dispatch};

pub(super) fn register_event_systems(builder: &mut DispatcherBuilder) {
    event_dispatch::<PlaceBlockEvent>(builder, &[]);
    event_dispatch::<CreuseBlocEvent>(builder, &[]);
}

/// L'avancement d'un creusement, pour une position du monde.
///
/// Volontairement **hors du bloc** : un bloc plein n'a pas la place de porter
/// sa propre progression — ses trois octets de donnees sont sa couleur, et
/// l'attribut `Damage` n'existe que sur les sprites. C'est donc un etat
/// serveur, et il est transitoire : rien n'est persiste, rien n'est
/// synchronise.
pub struct Creusement {
    creuseur: EcsEntity,
    avance: f32,
    /// Quand on en a entendu parler pour la derniere fois. Ce qui n'est plus
    /// alimente est oublie — voir [`DELAI_ABANDON_CREUSEMENT`].
    vu: f64,
    /// Le dernier quart franchi, pour n'emettre le retour visuel qu'aux
    /// passages plutot qu'a chaque tick.
    quart: u8,
}

/// Les creusements en cours, par position.
///
/// **Inseree explicitement dans `Server::new`**, pas laissee a `Write` : la
/// repartition des evenements va chercher ses donnees sans passer par
/// `System::setup`, donc `Default` n'est jamais appele et le serveur panique au
/// premier tick. Rien dans la compilation ne le dit — seul le jeu lance.
#[derive(Default)]
pub struct Creusements(HashMap<Vec3<i32>, Creusement>);

impl ServerEvent for CreuseBlocEvent {
    type SystemData<'a> = (
        WriteExpect<'a, BlockChange>,
        ReadExpect<'a, TerrainGrid>,
        WriteExpect<'a, Creusements>,
        Read<'a, Time>,
        Read<'a, ProgramTime>,
        ReadExpect<'a, EventBus<CreateItemDropEvent>>,
        ReadExpect<'a, EventBus<Outcome>>,
        ReadStorage<'a, comp::Inventory>,
        ReadStorage<'a, comp::ModeDeJeu>,
        crate::sys::msg::in_game::TerrainPersistenceData<'a>,
    );

    fn handle(
        events: impl ExactSizeIterator<Item = Self>,
        (
            mut block_change,
            terrain,
            mut creusements,
            time,
            program_time,
            create_item_drop_events,
            outcomes,
            inventories,
            modes,
            mut _terrain_persistence,
        ): Self::SystemData<'_>,
    ) {
        let mut create_item_drop_emitter = create_item_drop_events.emitter();
        let mut outcome_emitter = outcomes.emitter();
        let maintenant = time.0;

        // Ce qui n'a pas ete alimente depuis assez longtemps n'existe plus.
        // Lacher la souris, changer de cible ou se deconnecter passent tous par
        // ici, sans qu'aucun message d'arret ait a exister.
        creusements
            .0
            .retain(|_, c| maintenant - c.vu <= DELAI_ABANDON_CREUSEMENT);

        for ev in events {
            // En combat, le clic gauche frappe : rien n'arrive ici de bonne foi,
            // et ce qui y arriverait quand meme ne doit pas creuser.
            if modes.get(ev.entity).is_some_and(|mode| mode.combat) {
                continue;
            }
            let Ok(bloc) = terrain.get(ev.pos).copied() else {
                continue;
            };
            // L'outil vient de son emplacement, jamais de la main.
            let outil = inventories
                .get(ev.entity)
                .and_then(|inv| outil_pour(inv, &bloc));
            let Some(duree) = temps_de_casse(&bloc, outil) else {
                // Un fluide : il n'y a rien a creuser.
                continue;
            };
            let famille = outil.and_then(|item| item.tool_info());

            let entree = creusements.0.entry(ev.pos).or_insert(Creusement {
                creuseur: ev.entity,
                avance: 0.0,
                vu: maintenant,
                quart: 0,
            });
            // Un second joueur sur le meme bloc reprend a zero. Partager
            // l'avancement demanderait de decider a qui revient le butin ; on
            // le decidera le jour ou la question se pose.
            if entree.creuseur != ev.entity {
                entree.creuseur = ev.entity;
                entree.avance = 0.0;
                entree.quart = 0;
            }

            // Le temps ecoule depuis qu'on a eu des nouvelles, plafonne : un
            // trou dans les messages ne doit pas offrir un bloc d'un coup.
            let ecoule = (maintenant - entree.vu).clamp(0.0, DELAI_ABANDON_CREUSEMENT);
            entree.vu = maintenant;
            if duree > 0.0 {
                entree.avance += ecoule as f32 / duree;
            } else {
                entree.avance = 1.0;
            }

            // Un eclat a chaque quart franchi, et pas a chaque tick : le
            // joueur voit que son geste porte sans qu'on lui jette une gerbe
            // par image. `stage_changed: false` est le palier leger de
            // Veloren — dix particules et le coup sourd, la ou `true` en
            // envoie trente et le coup fort, qu'on garde pour la rupture.
            let quart = (entree.avance * 4.0) as u8;
            if quart > entree.quart && entree.avance < 1.0 {
                entree.quart = quart;
                outcome_emitter.emit(Outcome::DamagedBlock {
                    pos: ev.pos,
                    tool: famille,
                    stage_changed: false,
                });
            }

            if entree.avance < 1.0 {
                continue;
            }

            let vide = bloc.into_vacant();
            if block_change.try_set(ev.pos, vide).is_none() {
                // La position a deja ete modifiee ce tick : on reessaiera au
                // suivant, l'avancement est garde.
                continue;
            }
            creusements.0.remove(&ev.pos);

            #[cfg(feature = "persistent_world")]
            if let Some(terrain_persistence) = _terrain_persistence.as_mut() {
                terrain_persistence.set_block(ev.pos, vide);
            }

            outcome_emitter.emit(Outcome::BreakBlock {
                pos: ev.pos,
                tool: famille,
                color: bloc.get_color(),
            });

            // Le seul endroit ou le grade verrouille quoi que ce soit : sous le
            // grade requis le bloc part quand meme, et rien ne tombe. C'est la
            // perte qui enseigne — un bloc qui resiste n'enseigne rien.
            if !lache_son_objet(&bloc, outil) {
                continue;
            }
            if let Some(asset) = bloc.kind().item_drop_asset() {
                let mut rng = rand::rng();
                create_item_drop_emitter.emit(CreateItemDropEvent {
                    // au centre du bloc retire, pas a son coin
                    pos: Pos(ev.pos.as_::<f32>() + Vec3::broadcast(0.5)),
                    vel: Vel(Vec3::zero()),
                    ori: Ori::from(Dir::random_2d(&mut rng)),
                    item: PickupItem::new(
                        comp::Item::new_from_asset_expect(asset),
                        *program_time,
                        true,
                    ),
                    loot_owner: None,
                });
            }
        }
    }
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
