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
    terrain::{TerrainChunkSize, cube, neighbors, uniform_idx_as_vec2, vec2_as_uniform_idx},
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

    // Les sites remettent `tree_density` et `spawn_rate` a zero autour d'eux, et
    // ces deux champs entrent dans la loi des biomes. Les couper rend ce compte
    // comparable a celui de la fenetre de reglages, qui ne peut pas les avoir :
    // les biomes decideraient des sites qui decideraient des biomes.
    //
    // **Il faut s'arreter a `WorldSim` pour cela.** `World::generate` appelle
    // `Civs::generate` sans condition — `seed_elements` ne gouverne pas les
    // sites —, si bien qu'un simple drapeau sur les options n'en enlevait
    // aucun. La premiere version de ce mode annoncait « sans sites » et en
    // gardait : c'est la comparaison avec la fenetre qui l'a dit, et rien
    // d'autre ne l'aurait dit.
    let sites = !std::env::args().any(|a| a == "--sans-sites");

    // La sonde des calottes tourne toujours — une mesure qui ne garde rien ne
    // garde rien. Le drapeau n'ajoute que le profil colonne par colonne, celui
    // qu'on lit quand un chiffre a bouge et qu'on veut savoir ou.
    let calottes = std::env::args().any(|a| a == "--calottes");

    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();

    if !sites {
        // Le mode de comparaison ne fait que compter : ni sites a placer, ni
        // colonne a sonder, donc pas de `World` du tout.
        let sim = veloren_world::sim::WorldSim::generate(
            42,
            WorldOpts {
                seed_elements: false,
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
        let map = sim.map_size_lg();
        println!(
            "Monde cubique : 6 faces de {} chunks · sans sites",
            cube::face_chunks(map)
        );
        println!();
        std::process::exit(les_biomes(&sim, None) as i32);
    }

    let (monde, index) = World::generate(
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
    echecs += les_sites(sim);
    echecs += les_biomes(sim, Some((&monde, index.as_index_ref())));
    echecs += les_calottes(sim, &monde, index.as_index_ref(), calottes);

    println!();
    if echecs == 0 {
        println!("Tout tient.");
    } else {
        println!("{echecs} mesure(s) en échec.");
        std::process::exit(1);
    }
}

/// **Les biomes, et ou aller les voir.**
///
/// Le comptage dit si un biome existe ; la coordonnee dit ou. Sans elle,
/// atteindre une tache de miasme ou une pastille de magie instable sur une
/// planete de six faces releve de la chance — et la graine est la meme en jeu
/// qu'ici, donc ce qui est ecrit la se retrouve avec un `/goto`.
///
/// On rend en echec un biome extreme qui n'existe nulle part : un seuil qui
/// n'ouvre sur rien n'est pas un reglage, c'est une panne muette.
fn les_biomes(
    sim: &veloren_world::sim::WorldSim,
    sonde: Option<(&World, veloren_world::index::IndexRef<'_>)>,
) -> u32 {
    use common::terrain::BiomeKind;
    use std::collections::HashMap;

    let map = sim.map_size_lg();
    let mut compte: HashMap<BiomeKind, u32> = HashMap::new();
    // La case la plus au centre de chaque biome extreme, approchee par la
    // premiere rencontree assez loin d'une couture pour qu'on y tienne.
    let mut ou: HashMap<BiomeKind, Vec2<i32>> = HashMap::new();
    let mut total = 0u32;

    for posi in vivantes(map) {
        let cle = uniform_idx_as_vec2(map, posi);
        let c = sim.get_idx(posi).expect("case vivante");
        let biome = c.get_biome();
        *compte.entry(biome).or_default() += 1;
        total += 1;
        // **Une case interieure, pas la premiere venue.** La premiere rencontree
        // tombe sur un bord de tache, souvent contre un coin de face ou les
        // blocs sont les plus petits — le pire endroit pour aller regarder.
        // Une case dont les huit voisins partagent le biome est au milieu de
        // quelque chose.
        // Et **loin d'un bord de face** : la premiere case interieure du marais
        // tombait sur un coin du patron, ou la distance de vue du client se
        // calcule negative et ou plus rien ne s'affiche. Un lieu ou l'on ne
        // voit rien n'est pas un lieu ou l'on va regarder.
        let f = cube::face_chunks(map);
        let au_large = cube::face_de_chunk(map, cle).is_some_and(|_| {
            let (u, v) = (cle.x.rem_euclid(f), cle.y.rem_euclid(f));
            let marge = (f / 8).max(4);
            u >= marge && v >= marge && u < f - marge && v < f - marge
        });
        if !ou.contains_key(&biome)
            && au_large
            && neighbors(map, posi)
                .all(|v| sim.get_idx(v).map_or(false, |c| c.get_biome() == biome))
        {
            ou.insert(biome, cle);
        }
    }

    let mut noms: Vec<_> = compte.iter().collect();
    noms.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("biomes :");
    for (biome, n) in noms {
        let part = 100.0 * *n as f64 / total as f64;
        let repere = ou.get(biome).map_or(String::new(), |cle| {
            let w = cle * TerrainChunkSize::RECT_SIZE.map(|e| e as i32);
            let c = sim.get_idx(vec2_as_uniform_idx(map, *cle)).unwrap();
            // Sur un biome marin — l'abysse, les deux calottes — le sol est le
            // fond, cent blocs sous la surface. On se pose sur ce qui porte,
            // pas sur ce qui est en dessous.
            let sol = c.alt.max(c.water_alt);
            match sonde {
                Some((monde, index)) => format!(
                    "  ·  /goto {} {} {:.0}  ·  {}",
                    w.x,
                    w.y,
                    sol + 12.0,
                    dessus(monde, index, w)
                ),
                None => String::new(),
            }
        });
        println!("  {biome:?} : {n} cases ({part:.1} %){repere}");
    }

    let manquants: Vec<_> = [
        BiomeKind::Abyss,
        BiomeKind::PackIce,
        BiomeKind::IceShelf,
        BiomeKind::Volcanic,
        BiomeKind::Miasma,
        BiomeKind::Arcane,
    ]
    .into_iter()
    .filter(|b| !compte.contains_key(b))
    .collect();
    if !manquants.is_empty() {
        println!("  biome(s) extreme(s) introuvable(s) : {manquants:?}");
    }
    u32::from(!manquants.is_empty())
}

/// **Le relief des deux calottes, mesuré en marchant** (D42).
///
/// Aucun instrument existant ne pouvait juger ce chantier : `empreinte` ne
/// hache jamais de chunks engendrés, et la fenêtre de réglages s'arrête au
/// `SimChunk`. Il fallait donc une mesure neuve — et surtout une mesure **de la
/// bonne forme**, la leçon la plus chère de D28.
///
/// D'où deux partis pris. Elle marche le long d'un méridien **bloc par bloc**,
/// du pôle vers le seuil : ce qui concerne un déplacement ne se vérifie qu'en
/// se déplaçant, et une marche entre deux colonnes voisines n'a de sens qu'à
/// cette résolution. Et elle interroge **les blocs, pas la formule** : demander
/// à la géométrie si sa crevasse est sèche serait lui demander de se relire.
/// C'est le générateur qu'on interroge, en descendant la colonne.
fn les_calottes(
    sim: &veloren_world::sim::WorldSim,
    monde: &World,
    index: veloren_world::index::IndexRef<'_>,
    verbeux: bool,
) -> u32 {
    use common::terrain::BlockKind;
    use veloren_world::sim::{Endroit, REGLAGES};

    let map = sim.map_size_lg();
    let r = &REGLAGES.relief;
    let onde = r.onde(&REGLAGES.calottes);

    // Les deux pôles, pris pour ce qu'ils sont : les cases de latitude extrême.
    let (mut nord, mut sud) = (None::<(Vec2<i32>, f32)>, None::<(Vec2<i32>, f32)>);
    for posi in vivantes(map) {
        let cle = uniform_idx_as_vec2(map, posi);
        let lat = sim.get_idx(posi).expect("case vivante").latitude;
        if nord.is_none_or(|(_, l)| lat > l) {
            nord = Some((cle, lat));
        }
        if sud.is_none_or(|(_, l)| lat < l) {
            sud = Some((cle, lat));
        }
    }

    let mut echecs = 0;
    println!("calottes :");

    for (nom, pole, seuil) in [
        ("banquise", nord.map(|(c, _)| c), REGLAGES.calottes.banquise),
        ("barrière", sud.map(|(c, _)| c), REGLAGES.calottes.barriere),
    ] {
        let Some(pole) = pole else {
            println!("  {nom} : pôle introuvable");
            echecs += 1;
            continue;
        };

        // **Ce que D42 a supposé et que rien n'impose.** Une calotte est « un
        // traitement de surface posé sur une colonne océanique », et le
        // soulèvement ne sait rien des calottes : ce qui se trouve sous une
        // est ce que l'érosion y a laissé. Une case de calotte émergée ne
        // porte aucune dalle — c'est un nunatak, et D42 les assume, mais un
        // continent polaire entier n'est plus un nunatak. On le compte plutôt
        // que de l'espérer.
        let nordique = nom == "banquise";
        let (mut cases, mut emergees) = (0u32, 0u32);
        for posi in vivantes(map) {
            let c = sim.get_idx(posi).expect("case vivante");
            if (nordique && c.latitude > seuil) || (!nordique && c.latitude < -seuil) {
                cases += 1;
                if c.alt > c.water_alt {
                    emergees += 1;
                }
            }
        }
        let part_emergee = 100.0 * f64::from(emergees) / f64::from(cases.max(1));

        // Du pôle vers le bord de la face, en ligne droite. La calotte tient
        // tout entière dans sa face — c'est ce que le plancher de 0,707
        // garantit —, donc ce segment la traverse sans jamais changer de face.
        // **Assez long pour dépasser le front**, et un peu plus. Le front du
        // sud est à `sin(lat) = 0,72`, soit `u = 0,964` sur le carré
        // gnomonique : à 96 % de la demi-face, pas à 94 %. Une portée d'une
        // demi-face moins un chunk s'arrêtait treize blocs trop tôt et la
        // falaise n'était jamais mesurée — la sonde disait « front non
        // atteint » et avait raison, mais pour la mauvaise raison.
        //
        // Déborder ne coûte rien : au-delà de l'arête on tombe sur une case
        // morte ou sur une autre face, et la colonne y est simplement sans
        // glace.
        let portee = (cube::face_chunks(map) / 2 + 1) * TerrainChunkSize::RECT_SIZE.x as i32;
        // Le centre de la case du pôle, pas son coin : le méridien part d'aussi
        // près du pôle que la grille le permet.
        let depart = pole * TerrainChunkSize::RECT_SIZE.map(|e| e as i32)
            + TerrainChunkSize::RECT_SIZE.map(|e| e as i32 / 2);

        // **Les quatre méridiens, et non le meilleur des quatre.** Deux raisons.
        //
        // Un pôle peut être émergé : le biome de calotte passe avant l'eau,
        // mais tout autant avant la terre, et D42 assume qu'il n'y a pas de
        // dalle sous un nunatak. Un méridien pris au hasard peut donc n'y
        // rencontrer aucune glace.
        //
        // Surtout, **une seule ligne radiale ne croise que quatre jointures de
        // plaques**, et le cœur d'une fente n'en occupe que huit blocs. Avec un
        // seul méridien, la bande du seuil n'en contenait aucune et la sonde
        // annonçait « zéro crevasse au seuil » — une affirmation sur le monde
        // tirée d'un défaut d'échantillonnage. Quatre lignes en croisent
        // quatre fois plus.
        //
        // Quatre axes et non huit, pour que deux colonnes voisines restent à un
        // bloc l'une de l'autre : une marche mesurée en diagonale ne serait plus
        // une marche.
        let axes = [
            Vec2::new(1, 0),
            Vec2::new(-1, 0),
            Vec2::new(0, 1),
            Vec2::new(0, -1),
        ];

        let mut bg = monde.sample_blocks();
        let mut colonnes = 0u32;
        let (mut bas, mut haut) = (f32::MAX, f32::MIN);
        let (mut lat_min, mut marge_min) = (f32::MAX, f32::MAX);
        let (mut marche_pire, mut marches) = (0.0f32, Vec::new());
        let mut jointures: Vec<f32>;
        let mut repere: Option<(Vec2<i32>, f32)> = None;
        let mut creux_repere = 0.0f32;
        // Le front, c'est la colonne de glace de plus faible latitude : c'est
        // la que la falaise tombe, et c'est le seul endroit ou elle se regarde.
        let mut front: Option<(Vec2<i32>, f32)> = None;
        let mut front_lat = f32::MAX;
        let (mut creux_pole, mut creux_seuil) = (0.0f32, 0.0f32);
        let mut eau_sur_glace = 0u32;
        let mut falaise = 0.0f32;

        for dir in axes {
            // Chaque méridien repart de zéro : une « marche » entre la fin d'une
            // ligne et le début de la suivante n'aurait aucun sens.
            let mut precedent: Option<f32> = None;

            for i in 0..portee {
                let wpos = depart + dir * i;
                let Some(zc) = bg.get_z_cache(wpos, index, None) else {
                    continue;
                };
                let lat = Endroit::nouveau(map, wpos.map(|e| e as f64)).latitude() as f32;
                let Some(calotte) = zc.sample.calotte else {
                    // La première colonne sans glace après une colonne qui en
                    // portait : c'est le front, et la falaise est ce qu'elle
                    // laissait dépasser de l'eau. **Sur de l'eau libre seulement** :
                    // une dalle qui s'arrête contre un nunatak n'est pas un front,
                    // c'est un rivage.
                    if let Some(sommet) = precedent.take()
                        && zc.sample.alt < zc.sample.water_level
                    {
                        falaise = falaise.max(sommet - zc.sample.water_level);
                    }
                    continue;
                };

                colonnes += 1;
                bas = bas.min(calotte.sommet);
                haut = haut.max(calotte.sommet);
                lat_min = lat_min.min(lat.abs());
                // **Le plafond du chunk doit avoir remonté avec le relief.** Une
                // marge négative, c'est de la glace rognée en silence.
                marge_min = marge_min.min(zc.get_z_limits().1 - calotte.sommet);

                if let Some(sommet) = precedent {
                    let marche = (calotte.sommet - sommet).abs();
                    marche_pire = marche_pire.max(marche);
                    marches.push(marche);
                }
                precedent = Some(calotte.sommet);

                // **La mesure qui réfute.** On descend la colonne depuis au-dessus
                // de la glace jusqu'au dessous de la dalle : un seul bloc d'eau
                // au-dessus du fond, et la fente est un chenal, pas une crevasse.
                if i % 4 == 0 {
                    // Depuis le **premier bloc qui doit être de la glace**, jamais
                    // depuis le fond lui-même : `fond` est fractionnaire, et un
                    // `as i32` qui tronque désignait le bloc d'en dessous — de
                    // l'eau parfaitement légitime, sous la dalle. Vingt-trois faux
                    // positifs pour une troncature, et la sonde accusait le
                    // générateur.
                    let (b, h) = (calotte.fond.ceil() as i32, calotte.sommet.ceil() as i32 + 6);
                    for z in b..h {
                        if bg
                            .get_with_z_cache(Vec3::new(wpos.x, wpos.y, z), Some(&zc))
                            .is_some_and(|b| b.kind() == BlockKind::Water)
                        {
                            eau_sur_glace += 1;
                        }
                    }
                }

                if verbeux && i % 16 == 0 {
                    println!(
                        "    {nom} +{i:4} · sin(lat) {:.4} · sommet {:7.1} · fond {:7.1}",
                        lat.abs(),
                        calotte.sommet,
                        calotte.fond
                    );
                }
            }
        }

        if colonnes == 0 {
            println!(
                "  {nom} : {cases} cases, {part_emergee:.0} % émergées — aucune dalle sur les \
                 quatre méridiens"
            );
            echecs += 1;
            continue;
        }

        // **La profondeur d'une fente est un écart local, jamais un écart à une
        // formule.** La mesurer contre `water_level + franc-bord` revenait à
        // redemander sa leçon à la loi : le dévers d'une plaque y entrait pour
        // autant que la crevasse, et une plaque basse comptait pour un creux.
        // On la prend donc contre le plus haut du voisinage — quarante blocs de
        // part et d'autre, soit un cinquième de plaque.
        // Et c'est une **part**, jamais un maximum. Les deux bandes n'ont pas
        // le même nombre de colonnes — 62 % de la banquise est émergée, et pas
        // uniformément —, si bien qu'un maximum comparait surtout deux tailles
        // d'échantillon. La part de colonnes profondément fendues ne s'y prête
        // pas.
        // Et elle se lit **au cœur d'une fente, pas partout**. Prise sur toutes
        // les colonnes, elle mesurait surtout combien de jointures la ligne
        // avait croisées — donc la chance, pas la loi. En ne retenant que les
        // colonnes que le bruit désigne comme un cœur, elle répond à la seule
        // question posée : là où une fente existe, est-elle plus profonde au
        // bord de la calotte qu'au pôle ?
        // **Les deux bandes se séparent à la latitude médiane, pas au milieu de
        // l'intervalle.** Coupée à mi-chemin du seuil et du pôle, la bande
        // extérieure de la banquise ne recevait que 463 colonnes sur 1 177 et,
        // les jointures étant distantes de deux cents blocs, elle n'en croisait
        // aucune : la sonde affichait « zéro cœur au seuil » et cela ne disait
        // rien du monde. Une médiane donne deux bandes de même effectif, donc
        // deux chiffres comparables.
        // **Et elle se prend en grille, pas sur l'étoile des méridiens.** Une
        // statistique d'un champ à deux dimensions ne se lit pas le long de
        // quatre rayons : ils convergent au pôle, si bien qu'une seule jointure
        // passant près de lui s'y compte quatre fois. C'est ce qui donnait
        // « 21 cœurs vers le pôle, zéro vers le seuil » sur deux bandes de même
        // effectif — un artefact de la forme de l'échantillon, pas un fait sur
        // le monde.
        //
        // La marche et la falaise, elles, ont besoin de colonnes voisines :
        // c'est le rayon qui les porte, et lui seul. Chaque mesure sur son
        // échantillon.
        //
        // La grille est celle des cases : une colonne au centre de chaque case
        // de calotte, donc un point tous les trente-deux blocs, uniformément.
        // Le cœur d'une fente n'y est pas résolu — il n'a que huit blocs de
        // large — mais il n'a pas à l'être : il suffit qu'on y tombe parfois.
        const VOISINAGE: i32 = 40;
        let coeur = r.crevasse_seuil + r.crevasse_largeur;
        let mut grille: Vec<(f32, Vec2<i32>, f32, f32, f32)> = Vec::new();
        let mut murs_d_eau = 0u32;
        for posi in vivantes(map) {
            let c = sim.get_idx(posi).expect("case vivante");
            if !((nordique && c.latitude > seuil) || (!nordique && c.latitude < -seuil)) {
                continue;
            }
            let w = uniform_idx_as_vec2(map, posi) * TerrainChunkSize::RECT_SIZE.map(|e| e as i32)
                + TerrainChunkSize::RECT_SIZE.map(|e| e as i32 / 2);
            let Some(zc) = bg.get_z_cache(w, index, None) else {
                continue;
            };
            let Some(calotte) = zc.sample.calotte else {
                continue;
            };
            grille.push((
                c.latitude.abs(),
                w,
                calotte.sommet,
                calotte.jointure,
                zc.sample.water_level,
            ));
        }

        jointures = grille.iter().map(|&(_, _, _, j, _)| j).collect();
        let mediane_lat = {
            let mut lats: Vec<f32> = grille.iter().map(|&(l, _, _, _, _)| l).collect();
            lats.sort_by(f32::total_cmp);
            lats.get(lats.len() / 2).copied().unwrap_or(seuil)
        };

        let (mut n_pole, mut n_seuil) = (0u32, 0u32);
        let (mut dalle_pole, mut dalle_seuil) = (0u32, 0u32);
        for &(lat, w, sommet, jointure, eau) in &grille {
            if lat < front_lat {
                front_lat = lat;
                front = Some((w, sommet));
            }
            if lat > mediane_lat {
                dalle_pole += 1;
            } else {
                dalle_seuil += 1;
            }
            if jointure < coeur {
                continue;
            }
            // Le plus haut du voisinage, pris aux quatre points cardinaux à
            // quarante blocs — un cinquième de plaque. Law-free : on ne
            // redemande rien à la géométrie, on regarde ce qu'elle a posé
            // autour.
            let haut_local = [
                Vec2::new(VOISINAGE, 0),
                Vec2::new(-VOISINAGE, 0),
                Vec2::new(0, VOISINAGE),
                Vec2::new(0, -VOISINAGE),
            ]
            .into_iter()
            .filter_map(|d| bg.get_z_cache(w + d, index, None))
            .filter_map(|zc| zc.sample.calotte.map(|c| c.sommet))
            .fold(sommet, f32::max);
            // **Un mur d'eau debout.** Une fente descend sous le niveau de la
            // mer et reste sèche, c'est tout son propos ; mais si elle atteint
            // le bord de la calotte, sa voisine n'a plus de dalle et se remplit
            // d'océan jusqu'à la surface. On obtient alors de l'air et de l'eau
            // côte à côte, à la même hauteur, séparés par rien — l'eau du
            // terrain ne coule pas. Vu à l'écran d'abord, mesuré ensuite.
            if sommet < eau {
                for d in [
                    Vec2::new(1, 0),
                    Vec2::new(-1, 0),
                    Vec2::new(0, 1),
                    Vec2::new(0, -1),
                ] {
                    // La voisine doit porter de l'eau *pour de bon* : sans
                    // dalle, mais aussi sous le niveau de la mer. Une fente qui
                    // borde une terre émergée n'a rien en face d'elle, et la
                    // compter donnait cinq faux positifs sur une calotte dont
                    // 62 % est émergée — la sonde accusait alors une géométrie
                    // qui n'avait rien fait.
                    if let Some(v) = bg.get_z_cache(w + d, index, None)
                        && v.sample.calotte.is_none()
                        && v.sample.alt < v.sample.water_level
                        && v.sample.water_level > sommet
                    {
                        murs_d_eau += 1;
                        break;
                    }
                }
            }

            let creux = haut_local - sommet;
            let (n, somme) = if lat > mediane_lat {
                (&mut n_pole, &mut creux_pole)
            } else {
                (&mut n_seuil, &mut creux_seuil)
            };
            *n += 1;
            *somme += creux;

            // **Le repère va sur la fente la plus profonde**, pas sur une
            // colonne quelconque de la dalle. Une plaque est plate sur deux
            // cents blocs : tombé au hasard dessus, on ne voit rien de ce que
            // l'étape a fait, et une capture d'écran de plaine blanche ne prouve
            // ni ne réfute quoi que ce soit.
            if creux > creux_repere {
                creux_repere = creux;
                repere = Some((w, sommet));
            }
        }
        creux_pole /= n_pole.max(1) as f32;
        creux_seuil /= n_seuil.max(1) as f32;

        marches.sort_by(f32::total_cmp);
        let mediane = marches[marches.len() / 2];
        println!(
            "  {nom} : {cases} cases dont {part_emergee:.0} % émergées · {colonnes} colonnes de \
             dalle · sommet de {bas:.0} à {haut:.0} · marche médiane {mediane:.2}, pire \
             {marche_pire:.1} · falaise de front {falaise:.0}"
        );
        println!(
            "    fente, au cœur : {creux_seuil:.1} bloc(s) de creux vers le seuil ({n_seuil} \
             cœurs sur {dalle_seuil} colonnes), {creux_pole:.1} vers le pôle ({n_pole} sur \
             {dalle_pole}), partage à sin(lat) {mediane_lat:.4} · plafond : marge minimale \
             {marge_min:.1}"
        );
        println!(
            "    eau au-dessus du fond de dalle : {eau_sur_glace} bloc(s) (attendu 0) · murs \
             d'eau au bord d'une fente : {murs_d_eau} (attendu 0) · min sin(lat) portant de la \
             glace : {lat_min:.4}"
        );
        if murs_d_eau > 0 {
            println!(
                "    ↳ une fente atteint le bord de la calotte : de l'air sec sous le niveau de \
                 la mer, l'océan debout à côté"
            );
            echecs += 1;
        }

        // **La distribution du bruit, parce qu'aucun seuil ne se devine.** Les
        // deux seuils de crête et de crevasse se lisent dans l'échelle brute du
        // Worley, qui n'est écrite nulle part — et qui n'est pas la même à plat
        // que sur un cube. Posés au jugement, ils ont donné une banquise sans
        // une seule crevasse : le bruit ne montait jamais jusqu'à eux.
        jointures.sort_by(f32::total_cmp);
        let q = |p: f32| jointures[((jointures.len() - 1) as f32 * p) as usize];
        println!(
            "    jointure : min {:.3} · q25 {:.3} · médiane {:.3} · q75 {:.3} · q95 {:.3} · max \
             {:.3}",
            jointures[0],
            q(0.25),
            q(0.50),
            q(0.75),
            q(0.95),
            jointures[jointures.len() - 1]
        );
        if let Some((w, sommet)) = front {
            println!(
                "    le front : /goto {} {} {:.0} · sin(lat) {front_lat:.4}",
                w.x,
                w.y,
                sommet + 2.0
            );
        }
        if let Some((w, sommet)) = repere {
            println!(
                "    aller voir : /goto {} {} {:.0} · une fente de {creux_repere:.0} blocs",
                w.x,
                w.y,
                sommet + 2.0
            );
        }

        if eau_sur_glace > 0 {
            println!("    ↳ une fente ouvre sur l'océan : c'est un chenal, pas une crevasse");
            echecs += 1;
        }
        if marge_min < 0.0 {
            println!("    ↳ le plafond du chunk rogne le relief en silence");
            echecs += 1;
        }
        // Le seuil de la banquise est haut : seule la barrière frôle le
        // plancher de la face. On le vérifie sur les deux, il ne coûte rien.
        if lat_min < veloren_world::sim::PLANCHER_FACE {
            println!(
                "    ↳ le front descend sous {:.3} : il enjambe les coutures de la face polaire",
                veloren_world::sim::PLANCHER_FACE
            );
            echecs += 1;
        }
        // Ne pas atteindre le front n'est pas une panne — la calotte peut être
        // émergée jusqu'à son bord —, mais c'est une mesure tronquée, et une
        // mesure tronquée qui se tait est un alibi.
        if falaise <= 0.0 {
            println!(
                "    ↳ front non atteint sur ce méridien (seuil {:.3} + onde {onde:.3}) : la \
                 falaise n'est pas mesurée",
                seuil
            );
        }
    }

    echecs
}

/// **Le bloc qu'on a sous les pieds, en ce point du monde.**
///
/// C'est la seule facon de repondre sans l'oeil : sur un ecran, la lumiere du
/// soir rend une cendre grise indiscernable d'une terre brune, et un biome
/// annonce dans le bandeau de debogage ne dit rien de ce que la colonne pose.
///
/// On le demande pour **tous** les biomes, pas seulement les extremes : c'est
/// ainsi qu'on a vu que sept biomes ordinaires posent la meme herbe, desert et
/// sommet de montagne compris.
fn dessus(monde: &World, index: veloren_world::index::IndexRef<'_>, wpos: Vec2<i32>) -> String {
    let mut bg = monde.sample_blocks();
    let Some(zc) = bg.get_z_cache(wpos, index, None) else {
        return "colonne absente".to_string();
    };
    let sol = zc.sample.alt.max(zc.sample.water_level).ceil() as i32;
    // On descend depuis au-dessus du sol : le premier bloc plein rencontre est
    // le dessus, quel qu'il soit — glace posee sur l'eau comprise.
    for dz in (-4..12).rev() {
        let p = Vec3::new(wpos.x, wpos.y, sol + dz);
        if let Some(b) = bg.get_with_z_cache(p, Some(&zc))
            && b.is_filled()
        {
            return format!("dessus {:?}", b.kind());
        }
    }
    "rien de plein".to_string()
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

/// **Les sites, les lieux nommés, et ce qu'ils font des coutures.**
///
/// Trois choses à savoir : que les six faces en portent — un tirage qui ignore
/// le patron gaspille 62,5 % de ses essais sur des emplacements morts —,
/// qu'aucun site n'enjambe un recollement, et que le nommage des biomes ait eu
/// lieu.
fn les_sites(sim: &veloren_world::sim::WorldSim) -> u32 {
    use std::collections::{HashMap, HashSet};

    let map = sim.map_size_lg();
    let mut par_face = [0u32; 6];
    let mut faces_du_site: HashMap<usize, HashSet<u8>> = HashMap::new();
    let mut pois = 0u32;
    let mut lieux = 0u32;
    let mut total_cases = 0u32;

    for posi in vivantes(map) {
        let cle = uniform_idx_as_vec2(map, posi);
        let face = cube::face_de_chunk(map, cle).expect("case vivante");
        let c = sim.get_idx(posi).expect("case vivante");
        total_cases += 1;
        if !c.sites.is_empty() {
            par_face[face as usize] += 1;
            for site in &c.sites {
                faces_du_site
                    .entry(site.id() as usize)
                    .or_default()
                    .insert(face);
            }
        }
        // `poi` ne désigne pas un point d'intérêt mais l'appartenance à un
        // biome nommé : le nommage l'écrit sur chaque case du biome.
        if c.poi.is_some() {
            pois += 1;
        }
        // `place` n'est écrit nulle part dans Veloren aujourd'hui — le champ
        // existe et reste vide, sur carte plate comme sur patron. On le compte
        // pour que son zéro soit su plutôt que découvert plus tard.
        if c.place.is_some() {
            lieux += 1;
        }
    }

    let total: u32 = par_face.iter().sum();
    let vides = par_face.iter().filter(|&&n| n == 0).count();
    let a_cheval = faces_du_site.values().filter(|f| f.len() > 1).count();

    println!("sites : {total} cases occupées, réparties {par_face:?} (faces sans site : {vides})");
    println!("  sites à cheval sur une couture : {a_cheval} (attendu 0)");
    println!(
        "biomes nommés : {pois} cases rattachées ({:.0} %) · champ `place` renseigné : {lieux}",
        100.0 * pois as f64 / total_cases as f64
    );

    u32::from(vides != 0) + u32::from(a_cheval != 0) + u32::from(pois == 0)
}
