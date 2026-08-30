//! Un monde cubique engendré pour de bon, et mesuré (D27, D37).
//!
//! Là où `cube_diag` n'interroge que le graphe, celui-ci fait tourner la
//! génération entière — bruit, océan, érosion, rivières — et regarde ce qui en
//! sort. C'est la mesure de l'étape de l'océan : sans exutoire commun, rien de
//! tout cela n'aboutit.
//!
//! ```bash
//! cargo run --release --example cube_monde -- --x-lg 7
//! ```

use common::{
    resources::MapKind,
    terrain::{TerrainChunkSize, cube, uniform_idx_as_vec2, vec2_as_uniform_idx},
    vol::RectVolSize,
};
use vek::*;
use veloren_world::{
    World,
    sim::{FileOpts, GenOpts, WorldOpts},
};

fn main() {
    let x_lg = std::env::args()
        .skip_while(|a| a != "--x-lg")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(7u32);
    // Pour bissecter : `--erosion 0` engendre le monde sans érosion du tout, ce
    // qui dit si une cassure vient du bruit ou du solveur.
    let erosion: f32 = std::env::args()
        .skip_while(|a| a != "--erosion")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);

    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let (monde, _index) = World::generate(
        42,
        WorldOpts {
            seed_elements: true,
            world_file: FileOpts::Generate(GenOpts {
                x_lg,
                y_lg: x_lg,
                map_kind: MapKind::Cube,
                erosion_quality: erosion,
                ..GenOpts::default()
            }),
            calendar: None,
        },
        &pool,
        &|_| {},
    );

    let sim = monde.sim();
    let map = sim.map_size_lg();
    assert!(map.est_cubique(), "la carte n'est pas cubique");
    let f = cube::face_chunks(map);
    println!(
        "Monde cubique : 6 faces de {f} chunks, rayon {:.0} blocs, érosion {erosion}",
        cube::rayon(map)
    );
    println!();

    let mut echecs = 0;
    echecs += la_mer(sim);
    echecs += tout_s_ecoule_vers_la_mer(sim);
    echecs += le_relief_aux_coutures(sim);

    println!();
    if echecs == 0 {
        println!("Tout tient.");
    } else {
        println!("{echecs} mesure(s) en échec.");
        std::process::exit(1);
    }
}

fn vivantes(map: common::terrain::MapSizeLg) -> impl Iterator<Item = usize> {
    (0..map.chunks_len())
        .filter(move |&posi| cube::chunk_vivant(map, uniform_idx_as_vec2(map, posi)))
}

/// La mer existe, et elle couvre une part du monde qu'on a choisie.
fn la_mer(sim: &veloren_world::sim::WorldSim) -> u32 {
    let map = sim.map_size_lg();
    let (mut mer, mut total) = (0u32, 0u32);
    let (mut bas, mut haut) = (f32::MAX, f32::MIN);

    for posi in vivantes(map) {
        let c = sim.get_idx(posi).expect("case vivante");
        total += 1;
        if c.alt <= c.water_alt {
            mer += 1;
        }
        bas = bas.min(c.alt);
        haut = haut.max(c.alt);
    }

    let part = 100.0 * mer as f64 / total as f64;
    println!("mer : {part:.1} % de la surface ({mer} cases sur {total})");
    println!("relief : de {bas:.0} à {haut:.0} blocs");
    // On vise `FRACTION_OCEAN`, et à taille représentative on l'obtient : 65,0 %
    // pour un quantile de 0,65, sur des faces de 128 chunks.
    //
    // La fourchette reste large parce qu'un **petit** monde ne s'y tient pas.
    // Sur des faces de 32 chunks, le même quantile donne 30,9 % : l'érosion y
    // soulève une part disproportionnée des terres, et un balayage du quantile
    // fait à cette taille suggérait de le porter à 0,72 — ce qui aurait noyé le
    // vrai monde et réduit les continents à des îlots. La leçon vaut d'être
    // gardée : un monde trop petit ne se comporte pas comme un monde réduit.
    u32::from(!(25.0..90.0).contains(&part))
}

/// **Toute case atteint la mer en descendant.**
///
/// C'est l'invariant que la carte plate tenait par son ourlet d'océan. Sans
/// lui, un bassin sans exutoire fait paniquer `get_lakes`, très loin de la
/// cause.
fn tout_s_ecoule_vers_la_mer(sim: &veloren_world::sim::WorldSim) -> u32 {
    let map = sim.map_size_lg();
    let taille = TerrainChunkSize::RECT_SIZE.x as i32;
    let (mut bloquees, mut pire) = (0u32, 0u32);

    for posi in vivantes(map) {
        let mut ici = posi;
        let mut pas = 0u32;
        loop {
            let c = sim.get_idx(ici).expect("case vivante");
            match c.downhill {
                // Pas d'aval : c'est une racine, donc la mer.
                None => break,
                Some(wpos) => {
                    ici = vec2_as_uniform_idx(map, wpos.map(|e| e.div_euclid(taille)));
                    pas += 1;
                    if pas > map.chunks_len() as u32 {
                        bloquees += 1;
                        break;
                    }
                },
            }
        }
        pire = pire.max(pas);
    }

    println!("écoulement : {bloquees} case(s) sans issue (attendu 0), plus long trajet {pire} pas");
    u32::from(bloquees != 0)
}

/// Le relief **fini** aux douze recollements — après érosion, rivières et tout.
///
/// C'est la mesure que l'étape du bruit ne pouvait qu'approcher : elle ne
/// regardait que le bruit, et l'érosion n'y était pas encore passée. Le chiffre
/// est donc attendu **mauvais** tant que le solveur balaie des lignes et des
/// colonnes ; il est ici pour qu'on sache de combien, et pour que l'étape
/// suivante ait quelque chose à faire baisser.
fn le_relief_aux_coutures(sim: &veloren_world::sim::WorldSim) -> u32 {
    let map = sim.map_size_lg();
    let f = cube::face_chunks(map);
    let bords: [(Vec2<i32>, fn(i32, i32) -> (i32, i32)); 4] = [
        (Vec2::new(1, 0), |f, w| (f - 1, w)),
        (Vec2::new(-1, 0), |_, w| (0, w)),
        (Vec2::new(0, 1), |f, w| (w, f - 1)),
        (Vec2::new(0, -1), |_, w| (w, 0)),
    ];

    // `decalage` = 0 mesure la vraie couture ; toute autre valeur mesure une
    // ligne **témoin**, parallèle à l'arête mais à l'intérieur de la face, où
    // aucun recollement n'a lieu. Sans ce témoin, un rapport de 1,5 ne dit rien :
    // on ignore ce que vaut le même rapport là où il n'y a rien à traverser.
    let mesurer = |decalage: i32| {
        let (mut couture, mut ordinaire) = (0.0f64, 0.0f64);
        let (mut pire, mut pire_ordinaire) = (0.0f64, 0.0f64);
        let mut n = 0u32;

        for face in 0..6u8 {
            for (sortie, place) in bords {
                for w in (f / 16)..(f - f / 16) {
                    let (cu, cv) = place(f, w);
                    let base = cube::cle_de_chunk(map, face, cu, cv);
                    let Some(dedans) = cube::voisin(map, base, -sortie * decalage) else {
                        continue;
                    };
                    let (Some(dehors), Some(avant)) = (
                        cube::voisin(map, dedans, sortie),
                        cube::voisin(map, dedans, -sortie),
                    ) else {
                        continue;
                    };
                    let alt = |cle: Vec2<i32>| sim.get(cle).map(|c| c.alt as f64);
                    let (Some(a), Some(b), Some(c)) = (alt(avant), alt(dedans), alt(dehors)) else {
                        continue;
                    };

                    let pas_couture = (c - b).abs();
                    let pas_ordinaire = (b - a).abs();
                    couture += pas_couture;
                    ordinaire += pas_ordinaire;
                    pire = pire.max(pas_couture);
                    pire_ordinaire = pire_ordinaire.max(pas_ordinaire);
                    n += 1;
                }
            }
        }
        (couture / ordinaire, pire / pire_ordinaire, n)
    };

    let (moyen, pire, n) = mesurer(0);
    let (t_moyen, t_pire, _) = mesurer(4);
    println!("relief aux coutures : {moyen:.2} en moyenne, {pire:.2} au pire ({n} pas)");
    println!("  témoin, 4 cases à l'intérieur : {t_moyen:.2} en moyenne, {t_pire:.2} au pire");

    // Pas de verdict : l'érosion vient seulement d'apprendre à traverser, et un
    // seuil posé maintenant ne mesurerait que l'endroit où on l'a posé. Le
    // témoin, lui, dit ce qu'un rapport « normal » vaut sur ce terrain.
    0
}
