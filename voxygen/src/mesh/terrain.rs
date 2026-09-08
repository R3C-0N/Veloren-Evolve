#![expect(clippy::clone_on_copy)] // TODO: fix after wgpu branch

use crate::{
    mesh::{
        MeshGen,
        greedy::{self, GreedyConfig, GreedyMesh},
    },
    render::{AltIndices, FluidVertex, Mesh, TerrainAtlasData, TerrainVertex, Vertex},
    scene::terrain::{BlocksOfInterest, DEEP_ALT, SHALLOW_ALT},
};
use common::{
    terrain::{Block, TerrainChunk},
    util::either_with,
    vol::{ReadVol, RectRasterableVol},
    volumes::vol_grid_2d::{CachedVolGrid2d, VolGrid2d},
};
use common_base::span;
use std::{collections::VecDeque, fmt::Debug, sync::Arc};
use tracing::{debug, error};
use vek::*;

#[derive(Clone, Copy, PartialEq)]
pub enum FaceKind {
    /// Opaque face that is facing something non-opaque; either
    /// water (Opaque(true)) or something else (Opaque(false)).
    Opaque(bool),
    /// Fluid face that is facing something non-opaque, non-tangible,
    /// and non-fluid (most likely air).
    Fluid,
}

pub const SUNLIGHT: u8 = 24;
pub const SUNLIGHT_INV: f32 = 1.0 / SUNLIGHT as f32;
pub const MAX_LIGHT_DIST: i32 = SUNLIGHT as i32;

/// Comment amorcer la file de propagation, une fois la descente solaire faite.
///
/// [`Amorcage::Exhaustif`] est l'algorithme d'origine : toute cellule atteinte
/// par la descente devient une graine. Il n'est plus employe en jeu — il
/// coutait de l'ordre du million de graines par colonne remaillee — mais il
/// reste ici comme **specification** de [`Amorcage::Frontiere`], auquel le test
/// d'equivalence le compare cellule par cellule. Le retirer rendrait
/// l'optimisation invérifiable.
#[derive(Clone, Copy, PartialEq)]
enum Amorcage {
    /// Les seules cellules qui ont quelque chose a ameliorer.
    Frontiere,
    /// Toutes. Lent, et c'est le point de comparaison.
    Exhaustif,
}

/// Le nombre de graines empilees depuis la derniere remise a zero, pour le seul
/// test.
///
/// Sans lui, le test d'equivalence prouverait que les deux amorcages donnent la
/// meme lumiere — y compris dans le cas ou la frontiere en empilerait autant
/// que l'exhaustif, c'est-a-dire ou l'optimisation ne ferait rien. Une mesure
/// qui ne bouge pas ne prouve pas.
#[cfg(test)]
static GRAINES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn calc_light<
    V: RectRasterableVol<Vox = Block> + ReadVol + Debug,
    L: Iterator<Item = (Vec3<i32>, u8)>,
>(
    is_sunlight: bool,
    amorcage: Amorcage,
    // When above bounds
    default_light: u8,
    bounds: Aabb<i32>,
    vol: &VolGrid2d<V>,
    lit_blocks: L,
) -> impl Fn(Vec3<i32>) -> f32 + 'static + Send + Sync + use<V, L> {
    span!(_guard, "calc_light");
    const UNKNOWN: u8 = 255;
    const OPAQUE: u8 = 254;

    let outer = Aabb {
        min: bounds.min - Vec3::new(SUNLIGHT as i32, SUNLIGHT as i32, 1),
        max: bounds.max + Vec3::new(SUNLIGHT as i32, SUNLIGHT as i32, 1),
    };

    let mut vol_cached = vol.cached();

    let mut light_map = vec![UNKNOWN; outer.size().product() as usize];
    let lm_idx = {
        let (w, h, _) = outer.clone().size().into_tuple();
        move |x, y, z| (w * h * z + h * x + y) as usize
    };
    // Light propagation queue
    let mut prop_que = lit_blocks
        .map(|(pos, light)| {
            let rpos = pos - outer.min;
            light_map[lm_idx(rpos.x, rpos.y, rpos.z)] = light.min(SUNLIGHT); // Brightest light
            (rpos.x as u8, rpos.y as u8, rpos.z as u16)
        })
        .collect::<VecDeque<_>>();
    // Start sun rays
    if is_sunlight {
        for x in 0..outer.size().w {
            for y in 0..outer.size().h {
                let mut light = SUNLIGHT as f32;
                for z in (0..outer.size().d).rev() {
                    let (min_light, attenuation) = vol_cached
                        .get(outer.min + Vec3::new(x, y, z))
                        .map_or((0, 0.0), |b| b.get_max_sunlight());

                    if light > min_light as f32 {
                        light = (light - attenuation).max(min_light as f32);
                    }

                    light_map[lm_idx(x, y, z)] = light.floor() as u8;

                    if light <= 0.0 {
                        break;
                    }
                }
            }
        }

        // Amorcer la file, la descente etant finie.
        //
        // Elle a rempli `light_map` *avant* que la propagation ne depile quoi
        // que ce soit, si bien qu'on peut choisir les graines en connaissance
        // de cause. Empiler chaque cellule atteinte — ce que faisait la
        // descente — mettait de l'ordre du million de graines par colonne
        // remaillee, dont la quasi-totalite n'avait rien a ameliorer : la
        // propagation est un point fixe max monotone, donc une graine dont tous
        // les voisins valent deja au moins sa valeur moins un ne peut jamais
        // servir, ni maintenant ni plus tard — toute cellule qui augmente est
        // reempilee et repropage d'elle-meme. Ce qui reste est la surface du
        // terrain et les bords d'ombre.
        //
        // **`UNKNOWN` et `OPAQUE` valent 255 et 254, donc plus que `SUNLIGHT`
        // :** le test `voisin < light - 1` est toujours faux pour eux, d'ou le
        // test explicite d'`UNKNOWN`. L'omettre perdrait exactement les
        // cellules par lesquelles `propagate` decouvre les blocs fluides encore
        // sombres, et l'interieur des cavites resterait noir.
        let (w, h, d) = outer.size().into_tuple();
        for z in 0..d {
            for x in 0..w {
                for y in 0..h {
                    let light = light_map[lm_idx(x, y, z)];
                    // Une valeur au-dela de `SUNLIGHT` est un marqueur, pas
                    // une lumiere.
                    //
                    // Une cellule a zero est ecartee, et c'est le seul point que
                    // le test d'equivalence ne couvre pas — les deux amorcages
                    // le partagent. L'argument : une graine a zero ne peut que
                    // faire passer un voisin d'`UNKNOWN` a `0` s'il est fluide,
                    // ou a `OPAQUE` sinon, sans jamais le reempiler
                    // (`*dest > 1` est faux). Or la fermeture rendue plus bas
                    // ramene `0`, `OPAQUE` et `UNKNOWN` a la meme valeur — zero.
                    // L'effet est donc inobservable.
                    if light == 0 || light > SUNLIGHT {
                        continue;
                    }

                    let a_faire = amorcage == Amorcage::Exhaustif
                        || [
                            (z + 1 < d).then(|| lm_idx(x, y, z + 1)),
                            (z > 0).then(|| lm_idx(x, y, z - 1)),
                            (y + 1 < h).then(|| lm_idx(x, y + 1, z)),
                            (y > 0).then(|| lm_idx(x, y - 1, z)),
                            (x + 1 < w).then(|| lm_idx(x + 1, y, z)),
                            (x > 0).then(|| lm_idx(x - 1, y, z)),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|i| {
                            let voisin = light_map[i];
                            voisin == UNKNOWN || voisin < light.saturating_sub(1)
                        });

                    if a_faire {
                        prop_que.push_back((x as u8, y as u8, z as u16));
                        #[cfg(test)]
                        GRAINES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
    }

    // Determines light propagation
    let propagate = |src: u8,
                     dest: &mut u8,
                     pos: Vec3<i32>,
                     prop_que: &mut VecDeque<_>,
                     vol: &mut CachedVolGrid2d<V>| {
        if *dest != OPAQUE {
            if *dest == UNKNOWN {
                if vol.get(outer.min + pos).ok().is_some_and(|b| b.is_fluid()) {
                    *dest = src.saturating_sub(1);
                    // Can't propagate further
                    if *dest > 1 {
                        prop_que.push_back((pos.x as u8, pos.y as u8, pos.z as u16));
                    }
                } else {
                    *dest = OPAQUE;
                }
            } else if *dest < src.saturating_sub(1) {
                *dest = src - 1;
                // Can't propagate further
                if *dest > 1 {
                    prop_que.push_back((pos.x as u8, pos.y as u8, pos.z as u16));
                }
            }
        }
    };

    // Propagate light
    while let Some(pos) = prop_que.pop_front() {
        let pos = Vec3::new(pos.0 as i32, pos.1 as i32, pos.2 as i32);
        let light = light_map[lm_idx(pos.x, pos.y, pos.z)];

        // Up
        // Bounds checking
        if pos.z + 1 < outer.size().d {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x, pos.y, pos.z + 1)).unwrap(),
                Vec3::new(pos.x, pos.y, pos.z + 1),
                &mut prop_que,
                &mut vol_cached,
            )
        }
        // Down
        if pos.z > 0 {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x, pos.y, pos.z - 1)).unwrap(),
                Vec3::new(pos.x, pos.y, pos.z - 1),
                &mut prop_que,
                &mut vol_cached,
            )
        }
        // The XY directions
        if pos.y + 1 < outer.size().h {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x, pos.y + 1, pos.z)).unwrap(),
                Vec3::new(pos.x, pos.y + 1, pos.z),
                &mut prop_que,
                &mut vol_cached,
            )
        }
        if pos.y > 0 {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x, pos.y - 1, pos.z)).unwrap(),
                Vec3::new(pos.x, pos.y - 1, pos.z),
                &mut prop_que,
                &mut vol_cached,
            )
        }
        if pos.x + 1 < outer.size().w {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x + 1, pos.y, pos.z)).unwrap(),
                Vec3::new(pos.x + 1, pos.y, pos.z),
                &mut prop_que,
                &mut vol_cached,
            )
        }
        if pos.x > 0 {
            propagate(
                light,
                light_map.get_mut(lm_idx(pos.x - 1, pos.y, pos.z)).unwrap(),
                Vec3::new(pos.x - 1, pos.y, pos.z),
                &mut prop_que,
                &mut vol_cached,
            )
        }
    }

    let min_bounds = Aabb {
        min: bounds.min - 1,
        max: bounds.max + 1,
    };

    // Minimise light map to reduce duplication. We can now discard light info
    // for blocks outside of the chunk borders.
    let mut light_map2 = vec![UNKNOWN; min_bounds.size().product() as usize];
    let lm_idx2 = {
        let (w, h, _) = min_bounds.clone().size().into_tuple();
        move |x, y, z| (w * h * z + h * x + y) as usize
    };
    for z in 0..min_bounds.size().d {
        for x in 0..min_bounds.size().w {
            for y in 0..min_bounds.size().h {
                let off = min_bounds.min - outer.min;
                light_map2[lm_idx2(x, y, z)] = light_map[lm_idx(x + off.x, y + off.y, z + off.z)];
            }
        }
    }

    drop(light_map);

    move |wpos| {
        let pos = wpos - min_bounds.min;
        let l = light_map2
            .get(lm_idx2(pos.x, pos.y, pos.z))
            .copied()
            .unwrap_or(default_light);

        if l != OPAQUE && l != UNKNOWN {
            l as f32 * SUNLIGHT_INV
        } else {
            0.0
        }
    }
}

#[expect(clippy::type_complexity)]
pub fn generate_mesh<'a>(
    vol: &'a VolGrid2d<TerrainChunk>,
    (range, max_texture_size, _boi): (Aabb<i32>, Vec2<u16>, &'a BlocksOfInterest),
) -> MeshGen<
    TerrainVertex,
    FluidVertex,
    TerrainVertex,
    (
        Aabb<f32>,
        TerrainAtlasData,
        Vec2<u16>,
        Arc<dyn Fn(Vec3<i32>) -> f32 + Send + Sync>,
        Arc<dyn Fn(Vec3<i32>) -> f32 + Send + Sync>,
        AltIndices,
        (f32, f32),
    ),
> {
    span!(
        _guard,
        "generate_mesh",
        "<&VolGrid2d as Meshable<_, _>>::generate_mesh"
    );

    // Find blocks that should glow
    // TODO: Search neighbouring chunks too!
    // let glow_blocks = boi.lights
    //     .iter()
    //     .map(|(pos, glow)| (*pos + range.min.xy(), *glow));
    /*  DefaultVolIterator::new(vol, range.min - MAX_LIGHT_DIST, range.max + MAX_LIGHT_DIST)
    .filter_map(|(pos, block)| block.get_glow().map(|glow| (pos, glow))); */

    // CHRONO JETABLE (etape 0) — a retirer avant le commit final.
    let t0 = std::time::Instant::now();

    let mut glow_blocks = Vec::new();

    // TODO: This expensive, use BlocksOfInterest instead
    let mut volume = vol.cached();
    for x in -MAX_LIGHT_DIST..range.size().w + MAX_LIGHT_DIST {
        for y in -MAX_LIGHT_DIST..range.size().h + MAX_LIGHT_DIST {
            for z in -1..range.size().d + 1 {
                let wpos = range.min + Vec3::new(x, y, z);
                volume
                    .get(wpos)
                    .ok()
                    .and_then(|b| b.get_glow())
                    .map(|glow| glow_blocks.push((wpos, glow)));
            }
        }
    }

    // Calculate chunk lighting (sunlight defaults to 1.0, glow to 0.0)
    // CHRONO JETABLE (etape 0)
    let t_lueur = t0.elapsed();

    let light = calc_light(
        true,
        Amorcage::Frontiere,
        SUNLIGHT,
        range,
        vol,
        core::iter::empty(),
    );
    // CHRONO JETABLE (etape 0)
    let t_soleil = t0.elapsed() - t_lueur;

    let glow = calc_light(
        false,
        Amorcage::Frontiere,
        0,
        range,
        vol,
        glow_blocks.into_iter(),
    );
    // CHRONO JETABLE (etape 0)
    let t_glow = t0.elapsed() - t_lueur - t_soleil;
    debug!(
        "chrono generate_mesh H={} balayage_lueur={:?} calc_light_soleil={:?} \
         calc_light_lueur={:?}",
        range.size().d,
        t_lueur,
        t_soleil,
        t_glow,
    );

    let (underground_alt, deep_alt) = vol
        .get_key(vol.pos_key((range.min + range.max) / 2))
        .map_or((0.0, 0.0), |c| {
            (c.meta().alt() - SHALLOW_ALT, c.meta().alt() - DEEP_ALT)
        });

    let mut opaque_limits = None::<Limits>;
    let mut fluid_limits = None::<Limits>;
    let mut air_limits = None::<Limits>;
    let flat_get = {
        span!(_guard, "copy to flat array");
        let (w, h, d) = range.size().into_tuple();
        // z can range from -1..range.size().d + 1
        let d = d + 2;
        let flat = {
            let mut volume = vol.cached();
            const AIR: Block = Block::empty();
            // TODO: Once we can manage it sensibly, consider using something like
            // Option<Block> instead of just assuming air.
            let mut flat = vec![AIR; (w * h * d) as usize];
            let mut i = 0;
            for x in 0..range.size().w {
                for y in 0..range.size().h {
                    for z in -1..range.size().d + 1 {
                        let wpos = range.min + Vec3::new(x, y, z);
                        let block = volume
                            .get(wpos)
                            .copied()
                            // TODO: Replace with None or some other more reasonable value,
                            // since it's not clear this will work properly with liquid.
                            .unwrap_or(AIR);
                        if block.is_opaque() {
                            opaque_limits = opaque_limits
                                .map(|l| l.including(z))
                                .or_else(|| Some(Limits::from_value(z)));
                        } else if block.is_liquid() {
                            fluid_limits = fluid_limits
                                .map(|l| l.including(z))
                                .or_else(|| Some(Limits::from_value(z)));
                        } else {
                            // Assume air
                            air_limits = air_limits
                                .map(|l| l.including(z))
                                .or_else(|| Some(Limits::from_value(z)));
                        };
                        flat[i] = block;
                        i += 1;
                    }
                }
            }
            flat
        };

        move |Vec3 { x, y, z }| {
            // z can range from -1..range.size().d + 1
            let z = z + 1;
            match flat.get((x * h * d + y * d + z) as usize).copied() {
                Some(b) => b,
                None => panic!("x {} y {} z {} d {} h {}", x, y, z, d, h),
            }
        }
    };

    // Constrain iterated area
    let (z_start, z_end) = match (air_limits, fluid_limits, opaque_limits) {
        (Some(air), Some(fluid), Some(opaque)) => air.three_way_intersection(fluid, opaque),
        (Some(air), Some(fluid), None) => air.intersection(fluid),
        (Some(air), None, Some(opaque)) => air.intersection(opaque),
        (None, Some(fluid), Some(opaque)) => fluid.intersection(opaque),
        // No interfaces (Note: if there are multiple fluid types this could change)
        (Some(_), None, None) | (None, Some(_), None) | (None, None, Some(_)) => None,
        (None, None, None) => {
            error!("Impossible unless given an input AABB that has a height of zero");
            None
        },
    }
    .map_or((0, 0), |limits| {
        let (start, end) = limits.into_tuple();
        let start = start.max(0);
        let end = end.clamp(start, range.size().d - 1);
        (start, end)
    });

    let max_size = max_texture_size;
    assert!(z_end >= z_start);
    let greedy_size = Vec3::new(range.size().w - 2, range.size().h - 2, z_end - z_start + 1);
    // NOTE: Terrain sizes are limited to 32 x 32 x 16384 (to fit in 24 bits: 5 + 5
    // + 14). FIXME: Make this function fallible, since the terrain
    // information might be dynamically generated which would make this hard
    // to enforce.
    assert!(greedy_size.x <= 32 && greedy_size.y <= 32 && greedy_size.z <= 16384);
    // NOTE: Cast is safe by prior assertion on greedy_size; it fits into a u16,
    // which always fits into a f32.
    let max_bounds: Vec3<f32> = greedy_size.as_::<f32>();
    // NOTE: Cast is safe by prior assertion on greedy_size; it fits into a u16,
    // which always fits into a usize.
    let greedy_size = greedy_size.as_::<usize>();
    let greedy_size_cross = Vec3::new(greedy_size.x - 1, greedy_size.y - 1, greedy_size.z);
    let draw_delta = Vec3::new(1, 1, z_start);

    let get_light = |_: &mut (), pos: Vec3<i32>| {
        if flat_get(pos).is_opaque() {
            0.0
        } else {
            light(pos + range.min)
        }
    };
    let get_ao = |_: &mut (), pos: Vec3<i32>| {
        if flat_get(pos).is_opaque() { 0.0 } else { 1.0 }
    };
    let get_glow = |_: &mut (), pos: Vec3<i32>| glow(pos + range.min);
    let get_color =
        |_: &mut (), pos: Vec3<i32>| flat_get(pos).get_color().unwrap_or_else(Rgb::zero);
    let get_kind = |_: &mut (), pos: Vec3<i32>| flat_get(pos).kind() as u8;
    let get_opacity = |_: &mut (), pos: Vec3<i32>| !flat_get(pos).is_opaque();
    let should_draw = |_: &mut (), pos: Vec3<i32>, delta: Vec3<i32>, _uv| {
        should_draw_greedy(pos, delta, &flat_get)
    };
    // NOTE: Conversion to f32 is fine since this i32 is actually in bounds for u16.
    let mesh_delta = Vec3::new(0.0, 0.0, (z_start + range.min.z) as f32);
    let create_opaque =
        |atlas_pos, pos, norm, meta| TerrainVertex::new(atlas_pos, pos + mesh_delta, norm, meta);
    let create_transparent = |_atlas_pos, pos: Vec3<f32>, norm| {
        // TODO: It *should* be possible to pull most of this code out of this function
        // and compute it per-chunk. For some reason, this doesn't work! If you,
        // dear reader, feel like giving it a go then feel free. For now
        // it's been kept as-is because I'm lazy and water vertices aren't nearly common
        // enough for this to matter much. If you want to test whether your
        // change works, look carefully at how waves interact between water
        // polygons in different chunks. If the join is smooth, you've solved the
        // problem!
        let key = vol.pos_key(range.min + pos.as_());
        let v00 = vol
            .get_key(key + Vec2::new(0, 0))
            .map_or(Vec3::zero(), |c| c.meta().river_velocity());
        let v10 = vol
            .get_key(key + Vec2::new(1, 0))
            .map_or(Vec3::zero(), |c| c.meta().river_velocity());
        let v01 = vol
            .get_key(key + Vec2::new(0, 1))
            .map_or(Vec3::zero(), |c| c.meta().river_velocity());
        let v11 = vol
            .get_key(key + Vec2::new(1, 1))
            .map_or(Vec3::zero(), |c| c.meta().river_velocity());
        let factor =
            (range.min + pos.as_()).map(|e| e as f32) / TerrainChunk::RECT_SIZE.map(|e| e as f32);
        let vel = Lerp::lerp(
            Lerp::lerp(v00, v10, factor.x.rem_euclid(1.0)),
            Lerp::lerp(v01, v11, factor.x.rem_euclid(1.0)),
            factor.y.rem_euclid(1.0),
        );
        FluidVertex::new(pos + mesh_delta, norm, vel.xy())
    };

    let mut greedy = GreedyMesh::<TerrainAtlasData, guillotiere::SimpleAtlasAllocator>::new(
        max_size,
        greedy::general_config(),
    );
    let mut opaque_deep = Vec::new();
    let mut opaque_shallow = Vec::new();
    let mut opaque_surface = Vec::new();
    let mut fluid_mesh = Mesh::new();
    greedy.push(GreedyConfig {
        data: (),
        draw_delta,
        greedy_size,
        greedy_size_cross,
        get_ao,
        get_light,
        get_glow,
        get_opacity,
        should_draw,
        push_quad: |atlas_origin, dim, origin, draw_dim, norm, meta: &FaceKind| match meta {
            FaceKind::Opaque(meta) => {
                let mut max_z = None;
                let mut min_z = None;
                let quad = greedy::create_quad(
                    atlas_origin,
                    dim,
                    origin,
                    draw_dim,
                    norm,
                    meta,
                    |atlas_pos, pos, norm, &meta| {
                        max_z = Some(max_z.map_or(pos.z, |z: f32| z.max(pos.z)));
                        min_z = Some(min_z.map_or(pos.z, |z: f32| z.min(pos.z)));
                        create_opaque(atlas_pos, pos, norm, meta)
                    },
                );
                let max_alt = mesh_delta.z + max_z.expect("quad had no vertices?");
                let min_alt = mesh_delta.z + min_z.expect("quad had no vertices?");

                if max_alt < deep_alt {
                    opaque_deep.push(quad);
                } else if min_alt > underground_alt {
                    opaque_surface.push(quad);
                } else {
                    opaque_shallow.push(quad);
                }
            },
            FaceKind::Fluid => {
                fluid_mesh.push_quad(greedy::create_quad(
                    atlas_origin,
                    dim,
                    origin,
                    draw_dim,
                    norm,
                    &(),
                    |atlas_pos, pos, norm, &_meta| create_transparent(atlas_pos, pos, norm),
                ));
            },
        },
        make_face_texel: |(col_light, kind): (&mut [u8; 4], &mut u8),
                          data: &mut (),
                          pos,
                          light,
                          glow,
                          ao| {
            *col_light = TerrainVertex::make_col_light(light, glow, get_color(data, pos), ao);
            *kind = get_kind(data, pos);
        },
    });

    let min_bounds = mesh_delta;
    let bounds = Aabb {
        min: min_bounds,
        max: max_bounds + min_bounds,
    };
    let (atlas_data, atlas_size) = greedy.finalize();

    let deep_end = opaque_deep.len()
        * if TerrainVertex::QUADS_INDEX.is_some() {
            4
        } else {
            6
        };
    let alt_indices = AltIndices {
        deep_end,
        underground_end: deep_end
            + opaque_shallow.len()
                * if TerrainVertex::QUADS_INDEX.is_some() {
                    4
                } else {
                    6
                },
    };
    let sun_occluder_z_bounds = (underground_alt.max(bounds.min.z), bounds.max.z);

    (
        opaque_deep
            .into_iter()
            .chain(opaque_shallow)
            .chain(opaque_surface)
            .collect(),
        fluid_mesh,
        Mesh::new(),
        (
            bounds,
            atlas_data,
            atlas_size,
            Arc::new(light),
            Arc::new(glow),
            alt_indices,
            sun_occluder_z_bounds,
        ),
    )
}

/// NOTE: Make sure to reflect any changes to how meshing is performanced in
/// [crate::scene::terrain::Terrain::skip_remesh].
pub fn should_draw_greedy(
    pos: Vec3<i32>,
    delta: Vec3<i32>,
    flat_get: impl Fn(Vec3<i32>) -> Block,
) -> Option<(bool, FaceKind)> {
    let from = flat_get(pos - delta);
    let to = flat_get(pos);
    // Don't use `is_opaque`, because it actually refers to light transmission
    let from_filled = from.is_filled();
    if from_filled == to.is_filled() {
        // Check the interface of liquid and non-tangible non-liquid (e.g. air).
        let from_liquid = from.is_liquid();
        if from_liquid == to.is_liquid() || from.is_filled() || to.is_filled() {
            None
        } else {
            // While liquid is not culled, we still try to keep a consistent orientation as
            // we do for land; if going from liquid to non-liquid,
            // forwards-facing; otherwise, backwards-facing.
            Some((from_liquid, FaceKind::Fluid))
        }
    } else {
        // If going from unfilled to filled, backward facing; otherwise, forward
        // facing.  Also, if either from or to is fluid, set the meta accordingly.
        Some((
            from_filled,
            FaceKind::Opaque(if from_filled {
                to.is_liquid()
            } else {
                from.is_liquid()
            }),
        ))
    }
}

/// 1D Aabr
#[derive(Copy, Clone, Debug)]
struct Limits {
    min: i32,
    max: i32,
}

impl Limits {
    fn from_value(v: i32) -> Self { Self { min: v, max: v } }

    fn including(mut self, v: i32) -> Self {
        if v < self.min {
            self.min = v
        } else if v > self.max {
            self.max = v
        }
        self
    }

    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    // Find limits that include the overlap of the two
    fn intersection(self, other: Self) -> Option<Self> {
        // Expands intersection by 1 since that fits our use-case
        // (we need to get blocks on either side of the interface)
        let min = self.min.max(other.min) - 1;
        let max = self.max.min(other.max) + 1;

        (min < max).then_some(Self { min, max })
    }

    // Find limits that include any areas of overlap between two of the three
    fn three_way_intersection(self, two: Self, three: Self) -> Option<Self> {
        let intersection = self.intersection(two);
        let intersection = either_with(self.intersection(three), intersection, Limits::union);
        either_with(two.intersection(three), intersection, Limits::union)
    }

    fn into_tuple(self) -> (i32, i32) { (self.min, self.max) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::terrain::{BlockKind, MapSizeLg, SpriteKind, TerrainChunkMeta, TerrainChunkSize};
    use common::vol::{RectVolSize, WriteVol};

    const ROCHE: Block = Block::new(BlockKind::Rock, Rgb::new(60, 60, 60));
    const FEUILLAGE: Block = Block::new(BlockKind::Leaves, Rgb::new(20, 90, 30));

    /// Un monde de 5x5 colonnes portant une colline, un surplomb, une poche
    /// scellee, un lac et une voute de feuillage.
    ///
    /// Aucune de ces formes n'est decorative ; chacune couvre un mode de
    /// defaillance de l'amorcage par la frontiere :
    ///
    /// | Forme | Ce qu'elle eprouve |
    /// |---|---|
    /// | Colline | un bord d'ombre franc |
    /// | Surplomb | la lumiere qui doit arriver lateralement sous un toit |
    /// | Poche scellee | le seul cas ou `propagate` doit decouvrir des blocs fluides encore `UNKNOWN` — c'est elle que le test explicite d'`UNKNOWN` protege |
    /// | Lac | l'atténuation **fractionnaire** de l'eau (0,4 par bloc), qui fait des paliers ou deux voisins ne differrent que de zero apres troncature |
    /// | Feuillage | un plancher de lumiere (`min_light` = 9) jouxtant du plein soleil, donc le gradient lateral le plus raide du lot |
    fn monde_temoin() -> VolGrid2d<TerrainChunk> {
        let map_size_lg = MapSizeLg::new(Vec2::new(10, 10)).expect("taille de carte valide");
        let vide = Arc::new(TerrainChunk::new(
            0,
            ROCHE,
            Block::empty(),
            TerrainChunkMeta::void(),
        ));
        let mut vol = VolGrid2d::new(map_size_lg, vide).expect("grille valide");

        let (sx, sy) = (
            TerrainChunkSize::RECT_SIZE.x as i32,
            TerrainChunkSize::RECT_SIZE.y as i32,
        );

        for cx in -2..=2 {
            for cy in -2..=2 {
                let mut chunk =
                    TerrainChunk::new(0, ROCHE, Block::empty(), TerrainChunkMeta::void());

                for x in 0..sx {
                    for y in 0..sy {
                        // Le sol, et une colline centree sur la colonne (0, 0).
                        let wx = cx * sx + x;
                        let wy = cy * sy + y;
                        let d = ((wx * wx + wy * wy) as f32).sqrt();
                        let sommet = 40 + (18.0 - d * 0.5).max(0.0) as i32;
                        for z in 0..sommet {
                            chunk.set(Vec3::new(x, y, z), ROCHE).expect("sol");
                        }

                        // Un surplomb : une dalle a z=60, air en dessous.
                        if (10..24).contains(&wx) && (10..24).contains(&wy) {
                            for z in 60..63 {
                                chunk.set(Vec3::new(x, y, z), ROCHE).expect("dalle");
                            }
                        }

                        // Une poche scellee, creusee dans la masse du sol.
                        if (4..12).contains(&wx) && (4..12).contains(&wy) {
                            for z in 20..28 {
                                chunk
                                    .set(Vec3::new(x, y, z), Block::empty())
                                    .expect("poche");
                            }
                        }

                        // Un lac : de l'eau posee dans le sol, air au-dessus.
                        if (0..10).contains(&wx) && (20..30).contains(&wy) {
                            for z in sommet - 7..sommet {
                                chunk
                                    .set(Vec3::new(x, y, z), Block::water(SpriteKind::Empty))
                                    .expect("lac");
                            }
                        }

                        // Une voute de feuillage, a bonne distance du sol.
                        if (24..32).contains(&wx) && (0..10).contains(&wy) {
                            for z in 52..55 {
                                chunk.set(Vec3::new(x, y, z), FEUILLAGE).expect("voute");
                            }
                        }
                    }
                }

                let _ = vol.insert(Vec2::new(cx, cy), Arc::new(chunk));
            }
        }

        vol
    }

    /// L'amorcage par la frontiere atteint le meme point fixe que l'amorcage
    /// exhaustif.
    ///
    /// C'est la seule chose qui distingue l'optimisation d'un pari : le gain se
    /// mesure au chronometre, mais la correction ne se voit pas a l'ecran — une
    /// cavite restee noire sur trois blocs ne saute pas aux yeux.
    ///
    /// **Ce que ce test ne prouve pas.** Le tour `is_sunlight = false` est une
    /// tautologie : l'amorcage n'est consulte qu'a l'interieur de
    /// `if is_sunlight`, donc les deux appels y executent le meme code. Il est
    /// garde parce qu'il eprouve quand meme le chemin de la lueur — un
    /// debordement d'indice sur les graines de `lit_blocks` y serait attrape —,
    /// mais il ne pourrait pas voir une erreur d'amorcage. Le seul tour
    /// porteur est `is_sunlight = true`.
    #[test]
    fn amorcage_frontiere_egale_exhaustif() {
        let vol = monde_temoin();

        // L'etendue que `mesh_worker` demande : la colonne (0, 0) elargie d'un
        // bloc, sur toute sa hauteur.
        let bounds = Aabb {
            min: Vec3::new(-1, -1, -2),
            max: Vec3::new(
                TerrainChunkSize::RECT_SIZE.x as i32 + 1,
                TerrainChunkSize::RECT_SIZE.y as i32 + 1,
                66,
            ),
        };

        for is_sunlight in [true, false] {
            let graines: Vec<(Vec3<i32>, u8)> = if is_sunlight {
                Vec::new()
            } else {
                // Une lanterne posee dans la poche scellee, et une a l'air
                // libre pres d'un bord de colonne.
                vec![(Vec3::new(7, 7, 24), SUNLIGHT), (Vec3::new(1, 30, 59), 12)]
            };
            let defaut = if is_sunlight { SUNLIGHT } else { 0 };

            use std::sync::atomic::Ordering::Relaxed;

            GRAINES.store(0, Relaxed);
            let frontiere = calc_light(
                is_sunlight,
                Amorcage::Frontiere,
                defaut,
                bounds,
                &vol,
                graines.clone().into_iter(),
            );
            let n_frontiere = GRAINES.swap(0, Relaxed);
            let exhaustif = calc_light(
                is_sunlight,
                Amorcage::Exhaustif,
                defaut,
                bounds,
                &vol,
                graines.clone().into_iter(),
            );
            let n_exhaustif = GRAINES.swap(0, Relaxed);

            // Le gain, et non seulement la correction. Sans cette assertion, une
            // frontiere qui empilerait tout passerait le test en beneficiant de
            // l'egalite qu'elle rendrait triviale.
            if is_sunlight {
                eprintln!("graines : frontiere = {n_frontiere}, exhaustif = {n_exhaustif}");
                // Mesure sur ce monde temoin : 8 309 contre 157 621, soit un
                // facteur 19. Le seuil est pose a 10 pour garder de la marge
                // sans cesser de mordre. **En jeu le facteur est plus grand :**
                // les graines exhaustives croissent avec le volume d'air, donc
                // avec la hauteur remaillee, tandis que la frontiere reste la
                // surface du terrain. Le monde temoin n'a que 70 niveaux, une
                // colonne de falaise creusee de grottes en a plusieurs
                // centaines.
                assert!(
                    n_exhaustif > 10 * n_frontiere,
                    "l'amorcage par la frontiere n'elague pas : {n_frontiere} graines contre \
                     {n_exhaustif}"
                );
            }

            let mut compares = 0u32;
            for z in bounds.min.z - 1..=bounds.max.z + 1 {
                for x in bounds.min.x - 1..=bounds.max.x + 1 {
                    for y in bounds.min.y - 1..=bounds.max.y + 1 {
                        let pos = Vec3::new(x, y, z);
                        // Egalite exacte assumee : les deux valeurs sortent du
                        // meme `u8` divise par la meme constante. Un ecart, ici,
                        // serait un ecart de point fixe, pas d'arrondi.
                        assert_eq!(
                            frontiere(pos),
                            exhaustif(pos),
                            "divergence en {pos:?} (is_sunlight = {is_sunlight})"
                        );
                        compares += 1;
                    }
                }
            }
            assert!(compares > 90_000, "l'etendue comparee est trop maigre");
        }
    }
}
