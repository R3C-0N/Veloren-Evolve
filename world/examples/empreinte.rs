//! L'empreinte d'un monde plat, pour l'oracle de non-régression (D37).
//!
//! Le solveur d'érosion fait 2 683 lignes qu'on ne relit pas. La seule preuve
//! solide qu'on ne l'a pas cassé en le rendant cubique, c'est que le monde
//! **plat** sorte identique — même graine, même empreinte.
//!
//! **Une empreinte par champ, et non une seule.** Un chiffre unique dit qu'il
//! y a une différence, jamais laquelle — et il a fallu savoir laquelle le jour
//! où la loi de température a changé. Le relief d'un côté, le climat de
//! l'autre ; et deux champs à part, marqués d'une étoile.
//!
//! `alt` et `basement` sortent de l'érosion, mais reçoivent ensuite une
//! ondulation de dunes et de sol qui dépend de la température, de la densité
//! d'arbres et de l'humidité. Ils bougent donc quand le climat bouge, sans que
//! le solveur ait rien fait. Ce que l'érosion écrit seule — `chaos`,
//! `water_alt`, `flux`, `downhill` — est ce qui doit rester figé au bit près.
//!
//! ```bash
//! cargo run --release --example empreinte -- --x-lg 8
//! ```

use common::{
    terrain::{TerrainChunkSize, uniform_idx_as_vec2},
    vol::RectVolSize,
};
use std::hash::{DefaultHasher, Hash, Hasher};
use vek::*;
use veloren_world::{
    World,
    sim::{FileOpts, GenOpts, WorldOpts},
    util::Sampler,
};

fn main() {
    let x_lg = std::env::args()
        .skip_while(|a| a != "--x-lg")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(8u32);

    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let (monde, index) = World::generate(
        42,
        WorldOpts {
            seed_elements: true,
            world_file: FileOpts::Generate(GenOpts {
                x_lg,
                y_lg: x_lg,
                ..GenOpts::default()
            }),
            calendar: None,
        },
        &pool,
        &|_| {},
    );

    let sim = monde.sim();
    assert_eq!(sim.map_size_lg().vec(), Vec2::new(x_lg, x_lg));

    // On hache les champs en bits : une moyenne noierait justement l'erreur
    // qu'on cherche. **Un hachage par champ**, et non un seul pour tous : un
    // chiffre unique dit qu'il y a une différence, jamais laquelle, et il a
    // fallu justement savoir laquelle le jour où la loi de température a
    // changé.
    // **Trois groupes, pas deux.** `alt` et `basement` sortent bien de
    // l'érosion, mais ils reçoivent ensuite une ondulation de dunes et de sol
    // qui dépend de la température, de la densité d'arbres et de l'humidité
    // (voir `SimChunk::generate`). Les ranger avec l'érosion faisait accuser
    // le solveur d'une dérive qui venait du climat.
    const RELIEF: [&str; 6] = ["chaos", "alt*", "basement*", "water_alt", "flux", "downhill"];
    const CLIMAT: [&str; 8] = [
        "temp",
        "humidity",
        "rockiness",
        "tree_density",
        "spawn_rate",
        "surface_veg",
        "cliff_height",
        "forest_kind",
    ];
    let mut hr: Vec<DefaultHasher> = (0..RELIEF.len()).map(|_| DefaultHasher::new()).collect();
    let mut hk: Vec<DefaultHasher> = (0..CLIMAT.len()).map(|_| DefaultHasher::new()).collect();
    let mut sommet = 0.0f32;
    for posi in 0..sim.map_size_lg().chunks_len() {
        let pos = uniform_idx_as_vec2(sim.map_size_lg(), posi);
        let c = sim.get(pos).expect("case de la carte");
        // Le relief : ce que le solveur d'érosion écrit. Il ne doit jamais
        // bouger tant qu'on ne touche pas au solveur lui-même.
        for (i, v) in [c.chaos, c.alt, c.basement, c.water_alt, c.flux]
            .into_iter()
            .enumerate()
        {
            v.to_bits().hash(&mut hr[i]);
        }
        c.downhill.hash(&mut hr[5]);
        // Le climat : ce qui se pose sur le relief, et qui a le droit de
        // changer quand la loi de température change.
        for (i, v) in [
            c.temp,
            c.humidity,
            c.rockiness,
            c.tree_density,
            c.spawn_rate,
            c.surface_veg,
            c.cliff_height,
        ]
        .into_iter()
        .enumerate()
        {
            v.to_bits().hash(&mut hk[i]);
        }
        (c.forest_kind as u32).hash(&mut hk[7]);
        sommet = sommet.max(c.alt);
    }

    println!(
        "monde plat {} x {} chunks · graine 42",
        sim.map_size_lg().chunks().x,
        sim.map_size_lg().chunks().y
    );
    println!("altitude maximale : {sommet:.3}");
    // L'étoile marque les deux champs que le climat a le droit de déplacer.
    for (nom, h) in RELIEF.iter().zip(hr) {
        println!("  relief · {nom:<12} {:016x}", h.finish());
    }
    for (nom, h) in CLIMAT.iter().zip(hk) {
        println!("  climat · {nom:<12} {:016x}", h.finish());
    }

    // Puis les colonnes, échantillonnées directement.
    //
    // Et surtout **pas** des chunks engendrés : `generate_chunk` n'est pas
    // reproductible d'une exécution à l'autre — trois lancers du même binaire
    // donnent trois empreintes. Les sites et les couches parcourent des tables
    // de hachage dont l'ordre change à chaque processus. Une empreinte qui
    // bouge toute seule n'est pas un oracle, c'est un alibi : elle aurait
    // accusé `column.rs` d'une faute qu'il n'a pas commise.
    let colonnes = monde.sample_columns();
    let mut hc = DefaultHasher::new();
    let mut hcc = DefaultHasher::new();
    let mut n = 0u64;
    // Sur **toute** la carte, et non sur une tache au centre : un premier
    // échantillon de 512 blocs de côté ne rencontrait aucune falaise, si bien
    // que l'oracle restait muet quand on cassait exprès leur calcul. Une mesure
    // qui ne bouge pas n'est pas une mesure qui prouve (D28).
    let cote = sim.map_size_lg().chunks().x as i32 * TerrainChunkSize::RECT_SIZE.x as i32;
    for x in (0..cote).step_by(97) {
        for y in (0..cote).step_by(89) {
            let wpos = Vec2::new(x, y);
            let Some(col) = colonnes.get((wpos, index.as_index_ref(), None)) else {
                continue;
            };
            // Meme partage qu'au-dessus : la forme du sol d'un cote, ce qui
            // l'habille de l'autre.
            for v in [
                col.alt,
                col.riverless_alt,
                col.basement,
                col.chaos,
                col.water_level,
                col.warp_factor,
                col.marble,
                col.marble_mid,
                col.marble_small,
                col.cliff_offset,
                col.cliff_height,
            ] {
                v.to_bits().hash(&mut hc);
            }
            for v in [
                col.tree_density,
                col.rock_density,
                col.temp,
                col.humidity,
                col.ice_depth,
            ] {
                v.to_bits().hash(&mut hcc);
            }
            col.surface_color
                .map(f32::to_bits)
                .into_array()
                .hash(&mut hcc);
            col.sub_surface_color
                .map(f32::to_bits)
                .into_array()
                .hash(&mut hcc);
            col.stone_col.into_array().hash(&mut hcc);
            col.snow_cover.hash(&mut hcc);
            n += 1;
        }
    }
    println!("empreinte du relief de {n} colonnes : {:016x}", hc.finish());
    println!("empreinte du climat de {n} colonnes : {:016x}", hcc.finish());
}
