#[cfg(feature = "persistent_world")]
use crate::TerrainPersistence;
use crate::{EditableSettings, Settings, client::Client};
use common::{
    comp::{
        Admin, AdminRole, Body, ControlEvent, Controller, ForceUpdate, Health, Ori, Player, Pos,
        Presence, PresenceKind, Scale, SkillSet, SpectatingEntity, Vel,
    },
    consts::{BUILD_RANGE_SERVER_SLACK, MAX_BUILD_RANGE},
    event::{self, EmitExt},
    event_emitters,
    link::Is,
    mounting::{Rider, VolumeRider},
    resources::{DeltaTime, PlayerPhysicsSetting, PlayerPhysicsSettings},
    slowjob::SlowJobPool,
    terrain::TerrainGrid,
    uid::IdMaps,
    vol::ReadVol,
};
use common_ecs::{Job, Origin, Phase, System};
use common_net::msg::{ClientGeneral, ServerGeneral};
use core::mem;
use rayon::prelude::*;
use specs::{Entities, Join, LendJoin, Read, ReadExpect, ReadStorage, Write, WriteStorage};
use std::{borrow::Cow, time::Instant};
use tracing::{debug, trace, warn};
use vek::*;

#[cfg(feature = "persistent_world")]
pub type TerrainPersistenceData<'a> = Option<Write<'a, TerrainPersistence>>;
#[cfg(not(feature = "persistent_world"))]
pub type TerrainPersistenceData<'a> = core::marker::PhantomData<&'a mut ()>;

// Les « ecritures rares » d'autrefois — un mutex autour de `BlockChange` et de
// la persistance — n'ont plus d'usager : casser un bloc n'est plus un instant
// decide ici mais un creusement accumule par `events::construction`, la ou
// l'inventaire est lisible. Ce systeme n'ecrit donc plus dans le terrain, et
// n'a plus a se serialiser contre ceux qui le font.

event_emitters! {
    struct Events[Emitters] {
        exit_ingame: event::ExitIngameEvent,
        request_site_info: event::RequestSiteInfoEvent,
        update_map_marker: event::UpdateMapMarkerEvent,
        client_disconnect: event::ClientDisconnectEvent,
        set_battle_mode: event::SetBattleModeEvent,
        place_block: event::PlaceBlockEvent,
        creuse_bloc: event::CreuseBlocEvent,
    }
}

/// Tout point du monde est constructible : la seule borne est le bras du
/// joueur.
///
/// C'est ce qui remplace la zone declaree d'autrefois, laquelle etait la seule
/// borne de portee du serveur — la retirer sans rien mettre a la place aurait
/// laisse poser un bloc a l'autre bout de la carte.
///
/// La mesure part des pieds, la ou le client vise depuis la camera : d'ou la
/// marge, qui couvre l'ecart de hauteur sans allonger la portee utile.
fn a_portee_de_construction(joueur: Option<Pos>, bloc: Vec3<i32>) -> bool {
    joueur.is_some_and(|Pos(joueur)| {
        let centre = bloc.as_::<f32>() + Vec3::broadcast(0.5);
        joueur.distance_squared(centre) <= (MAX_BUILD_RANGE + BUILD_RANGE_SERVER_SLACK).powi(2)
    })
}

impl Sys {
    #[expect(clippy::too_many_arguments)]
    fn handle_client_in_game_msg(
        emitters: &mut Emitters,
        entity: specs::Entity,
        client: &Client,
        maybe_presence: &mut Option<&mut Presence>,
        is_rider: &ReadStorage<'_, Is<Rider>>,
        is_volume_rider: &ReadStorage<'_, Is<VolumeRider>>,
        force_update: Option<&&mut ForceUpdate>,
        skill_set: &mut Option<Cow<'_, SkillSet>>,
        healths: &ReadStorage<'_, Health>,
        position: Option<&mut Pos>,
        spectating_entity: &mut Option<Option<common::uid::Uid>>,
        controller: Option<&mut Controller>,
        settings: &Read<'_, Settings>,
        player_physics_setting: Option<&mut PlayerPhysicsSetting>,
        server_physics_forced: bool,
        maybe_admin: &Option<&Admin>,
        time_for_vd_changes: Instant,
        msg: ClientGeneral,
        player_physics: &mut Option<(Pos, Vel, Ori)>,
    ) -> Result<(), crate::error::Error> {
        let presence = match maybe_presence.as_deref_mut() {
            Some(g) => g,
            None => {
                debug!(?entity, "client is not in_game, ignoring msg");
                trace!(?msg, "ignored msg content");
                return Ok(());
            },
        };
        // Releve avant le match : `position` est consomme par la synchronisation
        // de position plus bas, et la construction n'a besoin que de la valeur.
        let player_pos = position.as_deref().copied();
        match msg {
            // Go back to registered state (char selection screen)
            ClientGeneral::ExitInGame => {
                emitters.emit(event::ExitIngameEvent { entity });
                client.send(ServerGeneral::ExitInGameSuccess)?;
                *maybe_presence = None;
            },
            ClientGeneral::SetViewDistance(view_distances) => {
                let clamped_vds = view_distances.clamp(settings.max_view_distance);

                presence
                    .terrain_view_distance
                    .set_target(clamped_vds.terrain, time_for_vd_changes);
                presence
                    .entity_view_distance
                    .set_target(clamped_vds.entity, time_for_vd_changes);

                // Correct client if its requested VD is too high.
                if view_distances.terrain != clamped_vds.terrain {
                    client.send(ServerGeneral::SetViewDistance(clamped_vds.terrain))?;
                }
            },
            ClientGeneral::ControllerInputs(inputs) => {
                if presence.kind.controlling_char()
                    && let Some(controller) = controller
                {
                    controller.inputs.update_with_new(*inputs);
                }
            },
            ClientGeneral::ControlEvent(event) => {
                if presence.kind.controlling_char()
                    && let Some(controller) = controller
                {
                    // Skip respawn if client entity is alive
                    let skip_respawn = matches!(event, ControlEvent::Respawn)
                        && healths.get(entity).is_none_or(|h| !h.is_dead);

                    if !skip_respawn {
                        controller.push_event(event);
                    }
                }
            },
            ClientGeneral::ControlAction(event) => {
                if presence.kind.controlling_char()
                    && let Some(controller) = controller
                {
                    controller.push_action(event);
                }
            },
            ClientGeneral::PlayerPhysics {
                pos,
                vel,
                ori,
                force_counter,
            } => {
                if presence.kind.controlling_char()
                    && force_update
                        .is_none_or(|force_update| force_update.counter() == force_counter)
                    && healths.get(entity).is_none_or(|h| !h.is_dead)
                    && is_rider.get(entity).is_none()
                    && is_volume_rider.get(entity).is_none()
                    && !server_physics_forced
                    && player_physics_setting
                        .as_ref()
                        .is_none_or(|s| !s.server_authoritative_physics_optin())
                {
                    *player_physics = Some((pos, vel, ori));
                }
            },
            ClientGeneral::CreuseBloc(pos) => {
                // Le message ne dit plus « casse ce bloc » mais « je creuse
                // ici », et le client le redit a chaque tick tant qu'il tient
                // le clic. Le serveur accumule ailleurs et decide seul quand le
                // bloc cede : ici on ne verifie que la portee.
                //
                // Le mode de jeu n'est pas teste ici mais dans le gestionnaire
                // d'evenement, qui a deja l'inventaire sous la main : une regle,
                // un endroit.
                //
                // Le traitement part en evenement pour la meme raison que la
                // pose : il faut lire l'inventaire — l'emplacement d'outil
                // decide du tempo et du butin — et ce systeme tourne en
                // `par_join`.
                if a_portee_de_construction(player_pos, pos) {
                    emitters.emit(event::CreuseBlocEvent { entity, pos });
                }
            },
            ClientGeneral::PlaceBlock(pos, slot) => {
                // Poser marche dans les deux modes : c'est la barricade de
                // D35, et c'est ce qui distingue la pose du creusement.
                if a_portee_de_construction(player_pos, pos) {
                    // Le bloc n'est pas ecrit ici : le client n'envoie qu'un
                    // emplacement d'inventaire, et lire l'objet puis le retirer
                    // doit se faire d'un seul tenant. Ce systeme tourne en
                    // par_join, l'inventaire n'y est pas accessible en ecriture,
                    // et l'inventaire n'est de toute facon pas une ecriture
                    // rare.
                    emitters.emit(event::PlaceBlockEvent { entity, pos, slot });
                }
            },
            ClientGeneral::UnlockSkill(skill) => {
                // FIXME: How do we want to handle the error?  Probably not by swallowing it.
                let _ = skill_set
                    .as_mut()
                    .map(|skill_set| {
                        SkillSet::unlock_skill_cow(skill_set, skill, |skill_set| skill_set.to_mut())
                    })
                    .transpose();
            },
            ClientGeneral::RequestSiteInfo(id) => {
                emitters.emit(event::RequestSiteInfoEvent { entity, id });
            },
            ClientGeneral::RequestPlayerPhysics {
                server_authoritative,
            } => {
                if let Some(setting) = player_physics_setting {
                    setting.client_optin = server_authoritative;
                }
            },
            ClientGeneral::RequestLossyTerrainCompression {
                lossy_terrain_compression,
            } => {
                presence.lossy_terrain_compression = lossy_terrain_compression;
            },
            ClientGeneral::UpdateMapMarker(update) => {
                emitters.emit(event::UpdateMapMarkerEvent { entity, update });
            },
            ClientGeneral::SpectatePosition(pos) => {
                if let Some(admin) = maybe_admin
                    && admin.0 >= AdminRole::Moderator
                    && presence.kind == PresenceKind::Spectator
                    && let Some(position) = position
                {
                    position.0 = pos;
                }
            },
            ClientGeneral::SpectateEntity(uid) => {
                if let Some(admin) = maybe_admin
                    && admin.0 >= AdminRole::Moderator
                {
                    *spectating_entity = Some(uid);
                }
            },
            ClientGeneral::SetBattleMode(battle_mode) => {
                emitters.emit(event::SetBattleModeEvent {
                    entity,
                    battle_mode,
                });
            },
            ClientGeneral::RequestCharacterList
            | ClientGeneral::CreateCharacter { .. }
            | ClientGeneral::EditCharacter { .. }
            | ClientGeneral::DeleteCharacter(_)
            | ClientGeneral::Character(_, _)
            | ClientGeneral::Spectate(_)
            | ClientGeneral::TerrainChunkRequest { .. }
            | ClientGeneral::LodZoneRequest { .. }
            | ClientGeneral::ChatMsg(_)
            | ClientGeneral::Command(..)
            | ClientGeneral::Terminate
            | ClientGeneral::RequestPlugins(_) => {
                debug!("Kicking possibly misbehaving client due to invalid client in game request");
                emitters.emit(event::ClientDisconnectEvent(
                    entity,
                    common::comp::DisconnectReason::NetworkError,
                ));
            },
        }
        Ok(())
    }
}

/// This system will handle new messages from clients
#[derive(Default)]
pub struct Sys;
impl<'a> System<'a> for Sys {
    type SystemData = (
        Entities<'a>,
        Events<'a>,
        (
            ReadExpect<'a, TerrainGrid>,
            ReadExpect<'a, SlowJobPool>,
            ReadExpect<'a, EditableSettings>,
        ),
        (Read<'a, IdMaps>, Read<'a, DeltaTime>, Read<'a, Settings>),
        WriteStorage<'a, ForceUpdate>,
        ReadStorage<'a, Is<Rider>>,
        ReadStorage<'a, Is<VolumeRider>>,
        WriteStorage<'a, SkillSet>,
        ReadStorage<'a, Health>,
        ReadStorage<'a, Body>,
        ReadStorage<'a, Scale>,
        WriteStorage<'a, Pos>,
        WriteStorage<'a, Vel>,
        WriteStorage<'a, Ori>,
        WriteStorage<'a, Presence>,
        WriteStorage<'a, Client>,
        WriteStorage<'a, Controller>,
        WriteStorage<'a, SpectatingEntity>,
        Write<'a, PlayerPhysicsSettings>,
        ReadStorage<'a, Player>,
        ReadStorage<'a, Admin>,
    );

    const NAME: &'static str = "msg::in_game";
    const ORIGIN: Origin = Origin::Server;
    const PHASE: Phase = Phase::Create;

    fn run(
        _job: &mut Job<Self>,
        (
            entities,
            events,
            (terrain, slow_jobs, editable_settings),
            (id_maps, dt, settings),
            mut force_updates,
            is_rider,
            is_volume_rider,
            mut skill_sets,
            healths,
            bodies,
            scales,
            mut positions,
            mut velocities,
            mut orientations,
            mut presences,
            mut clients,
            mut controllers,
            mut spectating_entities,
            mut player_physics_settings_,
            players,
            admins,
        ): Self::SystemData,
    ) {
        let time_for_vd_changes = Instant::now();

        let player_physics_settings = &*player_physics_settings_;
        let mut deferred_updates = (
            &entities,
            &mut clients,
            (&mut presences).maybe(),
            players.maybe(),
            admins.maybe(),
            (&skill_sets).maybe(),
            (&mut positions).maybe(),
            (&mut velocities).maybe(),
            (&mut orientations).maybe(),
            (&mut controllers).maybe(),
            (&mut force_updates).maybe(),
        )
            .join()
            // NOTE: Required because Specs has very poor work splitting for sparse joins.
            .par_bridge()
            .map_init(
                || events.get_emitters(),
                |emitters, (
                    entity,
                    client,
                    mut maybe_presence,
                    maybe_player,
                    maybe_admin,
                    skill_set,
                    ref mut pos,
                    ref mut vel,
                    ref mut ori,
                    ref mut controller,
                    ref mut force_update,
                )| {
                    let old_player_physics_setting = maybe_player.map(|p| {
                        player_physics_settings
                            .settings
                            .get(&p.uuid())
                            .copied()
                            .unwrap_or_default()
                    });
                    let mut new_player_physics_setting = old_player_physics_setting;
                    let is_server_physics_forced = maybe_player.is_none_or(|p| editable_settings.server_physics_force_list.contains_key(&p.uuid()));
                    // If an `ExitInGame` message is received this is set to `None` allowing further
                    // ingame messages to be ignored.
                    let mut clearable_maybe_presence = maybe_presence.as_deref_mut();
                    let mut skill_set = skill_set.map(Cow::Borrowed);
                    let mut player_physics = None;
                    let mut spectating_entity = None;
                    let _ = super::try_recv_all(client, 2, |client, msg| {
                        Self::handle_client_in_game_msg(
                            emitters,
                            entity,
                            client,
                            &mut clearable_maybe_presence,
                            &is_rider,
                            &is_volume_rider,
                            force_update.as_ref(),
                            &mut skill_set,
                            &healths,
                            pos.as_deref_mut(),
                            &mut spectating_entity,
                            controller.as_deref_mut(),
                            &settings,
                            new_player_physics_setting.as_mut(),
                            is_server_physics_forced,
                            &maybe_admin,
                            time_for_vd_changes,
                            msg,
                            &mut player_physics,
                        )
                    });

                    if let Some((new_pos, new_vel, new_ori)) = player_physics
                        && let Some(old_pos) = pos.as_deref_mut()
                        && let Some(old_vel) = vel.as_deref_mut()
                        && let Some(old_ori) = ori.as_deref_mut()
                    {
                        enum Rejection {
                            TooFar { old: Vec3<f32>, new: Vec3<f32> },
                            TooFast { vel: Vec3<f32> },
                            InsideTerrain,
                        }

                        let rejection = if maybe_admin.is_some() {
                            None
                        } else {
                            // Reminder: review these frequently to ensure they're reasonable
                            const MAX_H_VELOCITY: f32 = 75.0;
                            const MAX_V_VELOCITY: std::ops::Range<f32> = -100.0..80.0;

                            'rejection: {
                                let is_velocity_ok = new_vel.0.xy().magnitude_squared() < MAX_H_VELOCITY.powi(2)
                                    && MAX_V_VELOCITY.contains(&new_vel.0.z);

                                if !is_velocity_ok {
                                    break 'rejection Some(Rejection::TooFast { vel: new_vel.0 });
                                }

                                // How far the player is permitted to stray from the correct position (perhaps due to
                                // latency problems).
                                const POSITION_THRESHOLD: f32 = 16.0;

                                // The position can either be sensible with respect to either the old or the new
                                // velocity such that we don't punish for edge cases after a sudden change
                                let is_position_ok = [old_vel.0, new_vel.0]
                                    .into_iter()
                                    .any(|ref_vel| {
                                        let rpos = new_pos.0 - old_pos.0;
                                        // Determine whether the change in position is broadly consistent with both
                                        // the magnitude and direction of the velocity, with appropriate thresholds.
                                        LineSegment3 {
                                            start: Vec3::zero(),
                                            end: ref_vel * dt.0,
                                        }
                                            .projected_point(rpos)
                                            // + 1.5 accounts for minor changes in position without corresponding
                                            // velocity like block hopping/snapping
                                            .distance_squared(rpos) < (rpos.magnitude() * 0.5 + 1.5 + POSITION_THRESHOLD).powi(2)
                                    });

                                if !is_position_ok {
                                    break 'rejection Some(Rejection::TooFar { old: old_pos.0, new: new_pos.0 });
                                }

                                // Checks that are only relevant if the position changed
                                if new_pos.0 != old_pos.0 {
                                    // Reject updates that would move the entity into terrain
                                    let scale = scales.get(entity).map_or(1.0, |s| s.0);
                                    let min_z = new_pos.0.z as i32;
                                    let height = bodies.get(entity).map_or(0.0, |b| b.height()) * scale;
                                    let head_pos_z = (new_pos.0.z + height) as i32;

                                    if !(min_z..=head_pos_z).any(|z| {
                                        let pos = new_pos.0.as_().with_z(z);

                                        terrain
                                            .get(pos)
                                            .is_ok_and(|block| block.is_fluid())
                                    }) {
                                        break 'rejection Some(Rejection::InsideTerrain);
                                    }
                                }

                                None
                            }
                        };

                        if let Some(rejection) = rejection {
                            // TODO: Log when false positives aren't generated often
                            let alias = maybe_player.map(|p| &p.alias);
                            match rejection {
                                Rejection::TooFar { old, new } => warn!("Rejected physics for player {alias:?} (new position {new:?} is too far from old position {old:?})"),
                                Rejection::TooFast { vel } => warn!("Rejected physics for player {alias:?} (new velocity {vel:?} is too fast)"),
                                Rejection::InsideTerrain => warn!("Rejected physics for player {alias:?}: Inside terrain."),
                            }

                            /*
                            // Perhaps this is overzealous?
                            if let Some(mut setting) = new_player_physics_setting.as_mut() {
                                setting.server_force = true;
                                warn!("Switching player {alias:?} to server-side physics");
                            }
                            */

                            // Reject the change and force the server's view of the physics state
                            force_update.as_mut().map(|fu| fu.update());
                        } else {
                            *old_pos = new_pos;
                            *old_vel = new_vel;
                            *old_ori = new_ori;
                        }
                    }

                    // Ensure deferred view distance changes are applied (if the
                    // requsite time has elapsed).
                    if let Some(presence) = maybe_presence {
                        presence.terrain_view_distance.update(time_for_vd_changes);
                        presence.entity_view_distance.update(time_for_vd_changes);
                    }

                    // Return the possibly modified skill set, and possibly modified server physics
                    // settings.
                    let skill_set_update = skill_set.and_then(|skill_set| match skill_set {
                        Cow::Borrowed(_) => None,
                        Cow::Owned(skill_set) => Some((entity, skill_set)),
                    });
                    // NOTE: Since we pass Option<&mut _> rather than &mut Option<_> to
                    // handle_client_in_game_msg, and the new player was initialized to the same
                    // value as the old setting , we know that either both the new and old setting
                    // are Some, or they are both None.
                    let physics_update = maybe_player.map(|p| p.uuid())
                        .zip(new_player_physics_setting
                             .filter(|_| old_player_physics_setting != new_player_physics_setting));
                     let spectating_entity_update = spectating_entity.map(|e| (entity, e));
                    (skill_set_update, spectating_entity_update, physics_update)
                },
            )
            // NOTE: Would be nice to combine this with the map_init somehow, but I'm not sure if
            // that's possible.
            .filter(|(x, y, z)| x.is_some() || y.is_some() || z.is_some())
            // NOTE: I feel like we shouldn't actually need to allocate here, but hopefully this
            // doesn't turn out to be important as there shouldn't be that many connected clients.
            // The reason we can't just use unzip is that the two sides might be different lengths.
            .collect::<Vec<_>>();
        let player_physics_settings = &mut *player_physics_settings_;
        // Deferred updates to skillsets and player physics.
        //
        // NOTE: It is an invariant that there is at most one client entry per player
        // uuid; since we joined on clients, it follows that there's just one update
        // per uuid, so the physics update is sound and doesn't depend on evaluation
        // order, even though we're not updating directly by entity or uid (note that
        // for a given entity, we process messages serially).
        deferred_updates.iter_mut().for_each(
            |(skill_set_update, spectating_entity_update, physics_update)| {
                if let Some((entity, new_skill_set)) = skill_set_update {
                    // We know this exists, because we already iterated over it with the skillset
                    // lock taken, so we can ignore the error.
                    //
                    // Note that we replace rather than just updating.  This is in order to avoid
                    // dropping here; we'll drop later on a background thread, in case skillsets are
                    // slow to drop.
                    skill_sets
                        .get_mut(*entity)
                        .map(|mut old_skill_set| mem::swap(&mut *old_skill_set, new_skill_set));
                }
                if let &mut Some((entity, spectating_uid)) = spectating_entity_update {
                    if let Some(uid) = spectating_uid
                        && let Some(spectated_entity) = id_maps.uid_entity(uid)
                    {
                        // We know this exists, so can ignore the error.
                        let _ =
                            spectating_entities.insert(entity, SpectatingEntity(spectated_entity));
                    } else {
                        spectating_entities.remove(entity);
                    }
                }
                if let &mut Some((uuid, player_physics_setting)) = physics_update {
                    // We don't necessarily know this exists, but that's fine, because dropping
                    // player physics is a no op.
                    player_physics_settings
                        .settings
                        .insert(uuid, player_physics_setting);
                }
            },
        );
        // Finally, drop the deferred updates in another thread.
        slow_jobs.spawn("CHUNK_DROP", move || {
            drop(deferred_updates);
        });
    }
}
