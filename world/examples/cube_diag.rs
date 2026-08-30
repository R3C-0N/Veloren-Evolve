//! Le graphe d'adjacence du patron de cube, mesuré (D27, D37).
//!
//! Ce diagnostic n'engendre aucun monde : il n'interroge que l'**adjacence**,
//! c'est-à-dire ce que l'étape en cours a effectivement changé. Un monde
//! complet ferait intervenir l'érosion, les rivières et les sites, qui ne
//! traversent pas encore les coutures — il mesurerait donc autre chose que ce
//! qu'on cherche à savoir, et le chiffre cesserait d'être opposable.
//!
//! ```bash
//! cargo run --release --example cube_diag -- --x-lg 8
//! ```

use common::terrain::{
    MapSizeLg, NEIGHBOR_DELTA, cube, neighbors, neighbors_indexed, uniform_idx_as_vec2,
    vec2_as_uniform_idx,
};
use vek::*;

fn main() {
    let x_lg = std::env::args()
        .skip_while(|a| a != "--x-lg")
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(6u32);

    let cube_map = MapSizeLg::nouvelle_cubique(Vec2::new(x_lg, x_lg))
        .expect("la grille doit être carrée et laisser deux niveaux à une face");
    let plate = MapSizeLg::new(Vec2::new(x_lg, x_lg)).expect("carte plate valide");

    let f = cube::face_chunks(cube_map);
    println!(
        "Patron de cube : 6 faces de {f} chunks d'arête, soit {} blocs",
        cube::face_blocs(cube_map)
    );
    println!(
        "Rayon {:.0} blocs · tour du monde {:.0} blocs",
        cube::rayon(cube_map),
        2.0 * std::f64::consts::PI * cube::rayon(cube_map)
    );
    println!(
        "Grille {} x {} cases, dont {} vivantes ({:.1} % mortes)",
        cube_map.chunks().x,
        cube_map.chunks().y,
        6 * (f as usize).pow(2),
        100.0 - 100.0 * 6.0 / 16.0
    );
    println!();

    let mut echecs = 0;
    echecs += voisinage(cube_map);
    echecs += aucun_voisin_mort(cube_map);
    echecs += les_cases_mortes_sont_isolees(cube_map);
    echecs += reciprocite(cube_map);
    echecs += le_mode_plat_n_a_pas_bouge(plate);

    println!();
    if echecs == 0 {
        println!("Tout tient.");
    } else {
        println!("{echecs} mesure(s) en échec.");
        std::process::exit(1);
    }
}

/// Toute case vivante a huit voisins, sauf les vingt-quatre coins qui en ont
/// sept.
fn voisinage(map: MapSizeLg) -> u32 {
    let f = cube::face_chunks(map);
    let (mut sept, mut autre) = (0u32, 0u32);

    for face in 0..6u8 {
        for cu in 0..f {
            for cv in 0..f {
                let cle = cube::cle_de_chunk(map, face, cu, cv);
                let posi = vec2_as_uniform_idx(map, cle);
                let n = neighbors(map, posi).count();
                let au_coin = (cu == 0 || cu == f - 1) && (cv == 0 || cv == f - 1);
                match (n, au_coin) {
                    (7, true) => sept += 1,
                    (8, false) => {},
                    _ => {
                        if autre < 5 {
                            println!(
                                "  ÉCHEC face {} case ({cu}, {cv}) : {n} voisins",
                                cube::NOMS[face as usize]
                            );
                        }
                        autre += 1;
                    },
                }
            }
        }
    }

    println!("voisinage : {sept} cases à sept voisins (attendu 24), {autre} anomalie(s)");
    u32::from(sept != 24) + u32::from(autre != 0)
}

/// Aucune case vivante n'a de voisin dans les dix emplacements morts du patron.
fn aucun_voisin_mort(map: MapSizeLg) -> u32 {
    let f = cube::face_chunks(map);
    let mut fautes = 0u32;

    for face in 0..6u8 {
        for cu in 0..f {
            for cv in 0..f {
                let cle = cube::cle_de_chunk(map, face, cu, cv);
                for posj in neighbors(map, vec2_as_uniform_idx(map, cle)) {
                    if !cube::chunk_vivant(map, uniform_idx_as_vec2(map, posj)) {
                        fautes += 1;
                    }
                }
            }
        }
    }

    println!("voisins morts : {fautes} (attendu 0)");
    u32::from(fautes != 0)
}

/// Les cases mortes n'ont aucun voisin : elles ne participent à rien.
///
/// C'est ce qui les rend inoffensives pour tout ce qui parcourt le graphe. Ce
/// n'est pas suffisant pour l'érosion — une composante isolée n'a pas
/// d'exutoire, et `get_lakes` s'en étrangle —, mais cela relève de l'océan, pas
/// de l'adjacence.
fn les_cases_mortes_sont_isolees(map: MapSizeLg) -> u32 {
    let mut fautes = 0u32;
    let mut mortes = 0u32;

    for posi in 0..map.chunks_len() {
        if !cube::chunk_vivant(map, uniform_idx_as_vec2(map, posi)) {
            mortes += 1;
            if neighbors(map, posi).count() != 0 {
                fautes += 1;
            }
        }
    }

    println!("cases mortes : {mortes}, dont {fautes} non isolée(s) (attendu 0)");
    u32::from(fautes != 0)
}

/// Le voisinage est **réciproque**, et le pas aller est l'opposé du pas retour
/// une fois transporté.
///
/// Sans réciprocité, un flux pourrait descendre une arête qui n'existe pas dans
/// l'autre sens, et l'accumulation ne se refermerait jamais.
fn reciprocite(map: MapSizeLg) -> u32 {
    let f = cube::face_chunks(map);
    let mut fautes = 0u32;

    for face in 0..6u8 {
        for cu in 0..f {
            for cv in 0..f {
                let cle = cube::cle_de_chunk(map, face, cu, cv);
                let posi = vec2_as_uniform_idx(map, cle);
                for (k, posj) in neighbors_indexed(map, posi) {
                    if !neighbors(map, posj).any(|p| p == posi) {
                        fautes += 1;
                        continue;
                    }
                    // Le pas rendu par l'indice mène bien où il dit.
                    let (dx, dy) = NEIGHBOR_DELTA[k];
                    if cube::voisin(map, cle, Vec2::new(dx, dy))
                        != Some(uniform_idx_as_vec2(map, posj))
                    {
                        fautes += 1;
                    }
                }
            }
        }
    }

    println!("réciprocité et pas de direction : {fautes} faute(s) (attendu 0)");
    u32::from(fautes != 0)
}

/// En topologie plate, l'adjacence est **exactement** celle d'avant : mêmes
/// cases, dans le même ordre.
///
/// C'est l'oracle de non-régression. Le solveur d'érosion fait 2 683 lignes
/// qu'on ne relit pas ; la seule preuve solide qu'on ne l'a pas cassé est que
/// le monde plat sorte identique.
fn le_mode_plat_n_a_pas_bouge(map: MapSizeLg) -> u32 {
    let taille = map.chunks();
    let mut fautes = 0u32;

    for posi in 0..map.chunks_len() {
        let pos = uniform_idx_as_vec2(map, posi);
        // La définition d'origine, recopiée telle quelle.
        let attendu: Vec<usize> = NEIGHBOR_DELTA
            .iter()
            .map(|&(x, y)| Vec2::new(pos.x + x, pos.y + y))
            .filter(|p| p.x >= 0 && p.y >= 0 && p.x < taille.x as i32 && p.y < taille.y as i32)
            .map(|p| vec2_as_uniform_idx(map, p))
            .collect();

        if neighbors(map, posi).collect::<Vec<_>>() != attendu {
            fautes += 1;
        }
    }

    println!(
        "oracle du mode plat : {fautes} case(s) divergente(s) sur {} (attendu 0)",
        map.chunks_len()
    );
    u32::from(fautes != 0)
}
