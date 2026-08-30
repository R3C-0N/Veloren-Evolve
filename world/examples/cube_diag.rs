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

use common::{
    terrain::{
        MapSizeLg, NEIGHBOR_DELTA, TerrainChunkSize, cube, neighbors, neighbors_indexed,
        uniform_idx_as_vec2, vec2_as_uniform_idx,
    },
    vol::RectVolSize,
};
use noise::{Fbm, MultiFractal, Perlin, SuperSimplex};
use vek::*;
use veloren_world::sim::{Bruit, Endroit};

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
    echecs += coutures(cube_map);
    echecs += le_chargement_s_accorde(cube_map);

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

/// **La continuité du bruit aux douze recollements.**
///
/// C'est la moitié bon marché de D27, et la mesure doit le montrer plutôt que
/// l'affirmer. Le long de chaque arête, on compare le pas du bruit *en travers
/// de la couture* au pas *une case à l'intérieur* : si la projection fait son
/// travail, les deux sont indiscernables.
///
/// Le chiffre seul ne prouverait rien — un petit nombre est petit. On mesure
/// donc aussi ce que le même bruit donnait **lu en coordonnées de grille**,
/// c'est-à-dire comme Veloren le lisait avant. C'est ce rapport-là qui rend la
/// mesure opposable : elle peut échouer.
fn coutures(map: MapSizeLg) -> u32 {
    let f = cube::face_chunks(map);
    let taille = TerrainChunkSize::RECT_SIZE.x as f64;

    // Trois bruits de caractères différents, aux échelles de la génération.
    let continents: Fbm<Perlin> = Fbm::new(1).set_octaves(8);
    let relief: Fbm<Perlin> = Fbm::new(2).set_octaves(3);
    let collines = SuperSimplex::new(3);
    let bruits: [(&str, &dyn Bruit, f64); 3] = [
        ("continents", &continents, 10_000.0),
        ("relief", &relief, 2_000.0),
        ("collines", &collines, 400.0),
    ];

    // Les quatre bords d'une face, et le pas qui en sort.
    let bords: [(Vec2<i32>, fn(i32, i32) -> (i32, i32)); 4] = [
        (Vec2::new(1, 0), |f, w| (f - 1, w)),
        (Vec2::new(-1, 0), |_, w| (0, w)),
        (Vec2::new(0, 1), |f, w| (w, f - 1)),
        (Vec2::new(0, -1), |_, w| (w, 0)),
    ];

    let mut echecs = 0;
    let mut mesures = 0u32;
    println!();
    println!("continuité aux coutures — pas en travers / pas à l'intérieur");
    println!("                     projete sur la sphere | lu dans la grille");

    for (nom, nz, echelle) in bruits {
        // On somme les pas, et on divise **à la fin**. Une moyenne de rapports
        // n'aurait rien dit : le pas ordinaire passe par zéro à chaque extremum
        // local du bruit, et le quotient y explose sans qu'aucune couture ne
        // soit en cause. C'est une mesure de la mauvaise forme (D28).
        let (mut couture, mut ordinaire) = (0.0f64, 0.0f64);
        let (mut pire, mut pire_ordinaire) = (0.0f64, 0.0f64);
        let (mut couture_g, mut ordinaire_g) = (0.0f64, 0.0f64);
        mesures = 0;

        for face in 0..6u8 {
            for (sortie, place) in bords {
                // Les coins sont exclus : trois faces s'y rejoignent, et la
                // notion de « la case d'en face » y perd son sens.
                for w in (f / 16)..(f - f / 16) {
                    let (cu, cv) = place(f, w);
                    let dedans = cube::cle_de_chunk(map, face, cu, cv);
                    let Some(dehors) = cube::voisin(map, dedans, sortie) else {
                        continue;
                    };
                    let Some(avant) = cube::voisin(map, dedans, -sortie) else {
                        continue;
                    };

                    let lu = |cle: Vec2<i32>| {
                        Endroit::nouveau(map, cle.map(|e| e as f64) * taille).lire(nz, echelle)
                    };
                    let pas_couture = (lu(dehors) - lu(dedans)).abs();
                    let pas_ordinaire = (lu(dedans) - lu(avant)).abs();
                    couture += pas_couture;
                    ordinaire += pas_ordinaire;
                    pire = pire.max(pas_couture);
                    pire_ordinaire = pire_ordinaire.max(pas_ordinaire);

                    // Le même bruit, lu comme Veloren le lisait : en
                    // coordonnées de grille, à plat. `en2` est exactement ce
                    // chemin-là.
                    let brut = |cle: Vec2<i32>| {
                        let w = cle.map(|e| e as f64) * taille / echelle;
                        nz.en2([w.x, w.y])
                    };
                    couture_g += (brut(dehors) - brut(dedans)).abs();
                    ordinaire_g += (brut(dedans) - brut(avant)).abs();
                    mesures += 1;
                }
            }
        }

        let rapport = couture / ordinaire;
        let rapport_g = couture_g / ordinaire_g;
        // Le pire pas en travers, rapporte au **pire** pas ordinaire, et non a
        // sa moyenne : comparer un maximum a une moyenne est la meme faute de
        // forme que la moyenne de rapports. La moyenne repond d'un decalage
        // general, ce pire-ci d'une cassure isolee.
        let pire_relatif = pire / pire_ordinaire;
        println!(
            "  {nom:<11} moyen {rapport:>6.3} · pire isole {pire_relatif:>6.2}  |  moyen              {rapport_g:>7.1}",
        );

        if !(0.5..1.5).contains(&rapport) || pire_relatif > 2.0 {
            println!("    ECHEC : la couture se voit dans le bruit");
            echecs += 1;
        }
    }

    println!("  ({mesures} pas le long des 12 recollements, coins exclus)");
    echecs
}

/// **Ce que le client demande, le serveur doit l'accepter.**
///
/// Les deux moitiés du chargement ne parlent pas la même langue. Le client
/// énumère les chunks en **marchant** sur la surface — un balayage en anneaux
/// manque des régions près d'un coin. Le serveur, lui, décide d'envoyer sur une
/// **distance 3D** : à travers une couture, une différence de coordonnées vaut
/// la moitié de la carte.
///
/// Il faut donc qu'un chunk atteint en `d` pas soit à moins de `d` chunks dans
/// le monde, sans quoi le client réclamerait un terrain que le serveur refuse —
/// et le trou resterait ouvert.
fn le_chargement_s_accorde(map: MapSizeLg) -> u32 {
    const PORTEE: usize = 12;
    let f = cube::face_chunks(map);
    let taille = TerrainChunkSize::RECT_SIZE.x as f64;

    // Au bord, au coin, et en plein milieu pour comparaison.
    let lieux = [
        ("centre de face", cube::cle_de_chunk(map, 1, f / 2, f / 2)),
        ("milieu d'arête", cube::cle_de_chunk(map, 1, f - 1, f / 2)),
        ("coin", cube::cle_de_chunk(map, 1, f - 1, f - 1)),
    ];

    let mut fautes = 0u32;
    for (nom, depart) in lieux {
        let centre_3d = |cle: Vec2<i32>| {
            cube::direction(map, (cle.map(|e| e as f64) + 0.5) * taille).expect("case vivante")
        };
        let origine = centre_3d(depart);
        let rayon = cube::rayon(map);

        let mut vus: std::collections::HashSet<Vec2<i32>> =
            std::collections::HashSet::from_iter([depart]);
        let mut courante = vec![depart];
        let mut faces: std::collections::HashSet<u8> =
            std::collections::HashSet::from_iter(cube::face_de_chunk(map, depart));
        let mut pire = 0.0f64;

        for profondeur in 1..=PORTEE {
            let mut suivante = Vec::new();
            for &cle in &courante {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some(v) = cube::voisin(map, cle, Vec2::new(dx, dy))
                        && vus.insert(v)
                    {
                        suivante.push(v);
                        faces.extend(cube::face_de_chunk(map, v));
                        // La distance du monde, en chunks, comme le serveur la
                        // mesure.
                        let d = (centre_3d(v) - origine).magnitude() * rayon / taille;
                        pire = pire.max(d / profondeur as f64);
                    }
                }
            }
            courante = suivante;
        }

        // Un pas de grille ne dépasse jamais un chunk du monde : un chunk
        // atteint en `d` pas est donc à moins de `d` chunks.
        let accord = pire <= 1.0;
        println!(
            "chargement · {nom:<15} : {} chunks sur {} face(s), pire distance/profondeur \
             {pire:.3}{}",
            vus.len(),
            faces.len(),
            if accord { "" } else { "  ÉCHEC" }
        );
        if !accord {
            fautes += 1;
        }
    }
    fautes
}
