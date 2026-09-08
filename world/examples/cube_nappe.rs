//! La nappe lointaine, mesurée (D27, D28).
//!
//! La nappe de LOD est le dernier endroit du rendu qui balayait un rectangle :
//! sa grille `[-1, 1]²` était étirée jusqu'à couvrir tout le patron, faces
//! mortes comprises. Elle paramètre désormais un **disque géodésique** centré
//! sur le joueur, fermé à l'horizon.
//!
//! Ce diagnostic **rejoue en Rust la chaîne du vertex shader**, ligne pour
//! ligne : `splay_cube` → carte exponentielle → face → table inverse → lecture
//! bornée de l'atlas. C'est le même procédé que `conforme::le_shader_lit_la_-
//! meme_table` — le CPU et le GPU doivent voir la même planète, sinon le relief
//! dessiné et le relief simulé se décalent.
//!
//! **Chaque mesure porte son témoin.** Une valeur seule ne prouve rien : on
//! montre à chaque fois ce que la version fautive rendrait sur les mêmes
//! données. Une mesure qui ne distingue pas les deux est une mesure de la
//! mauvaise forme, et elle sert d'alibi.
//!
//! ```bash
//! cargo run --release --example cube_nappe -- --x-lg 10
//! ```

use common::{
    spiral::Spiral2d,
    terrain::{MapSizeLg, TerrainChunkSize, cube},
    vol::RectVolSize,
};
use vek::*;

fn main() {
    let arg = |nom: &str, defaut: f64| {
        std::env::args()
            .skip_while(|a| a != nom)
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaut)
    };
    let x_lg = arg("--x-lg", 10.0) as u32;
    let detail = arg("--detail", 250.0) as u32;

    let map = MapSizeLg::nouvelle_cubique(Vec2::new(x_lg, x_lg))
        .expect("la grille doit être carrée et laisser deux niveaux à une face");

    let rayon = cube::rayon(map);
    let fc = cube::face_chunks(map);
    println!(
        "Patron de cube : 6 faces de {fc} chunks, rayon {rayon:.0} blocs, tour du monde {:.0}",
        std::f64::consts::TAU * rayon
    );

    // Une caméra à cinquante blocs, un monde dont les sommets montent à 500.
    let h_cam = 50.0;
    let h_max = 500.0;
    let portee = cube::horizon(map, h_cam, h_max);
    let vd = 2080.0_f64.min(rayon * portee / 1.05);
    println!(
        "Horizon à {h_cam:.0} blocs : {:.0} blocs · portée totale {:.3} rad, soit {:.0} blocs \
         d'arc · distance de vue {vd:.0}",
        rayon * cube::horizon(map, h_cam, 0.0),
        portee,
        rayon * portee
    );
    println!();

    let mut echecs = 0;
    echecs += l_aller_retour_de_direction(map, rayon);
    echecs += aucun_sommet_sur_une_case_morte(map, rayon, portee, vd, detail);
    echecs += la_couture_ne_fait_pas_sauter_l_altitude(map, rayon);
    echecs += le_rayon_de_la_nappe_reste_positif(map, rayon, portee, detail);
    echecs += la_calotte_suit_l_observateur(map, rayon);
    echecs += le_pont_de_nuages_est_une_coquille(rayon);
    echecs += le_champ_de_reperes_est_continu(rayon);
    echecs += le_profil_de_la_nappe(rayon, vd);

    println!();
    if echecs == 0 {
        println!("Aucun échec.");
    } else {
        println!("{echecs} échec(s).");
        std::process::exit(1);
    }
}

// --------------------------------------------------------------------------
// La chaîne du shader, rejouée
// --------------------------------------------------------------------------

/// `splay_cube`, mot pour mot comme dans `include/lod.glsl`.
fn splay_cube(pos: Vec2<f64>, rayon: f64, portee: f64, vd: f64) -> Vec2<f64> {
    let echelle = rayon * portee;
    let lod_dist = vd * 0.95 / echelle;
    let dist = pos.x.abs() + pos.y.abs();
    let etirement = (dist.powf(5.5) * 0.75 + dist * 0.25) * (1.0 - lod_dist) + lod_dist;
    let etale = pos * etirement * echelle;
    let r = etale.magnitude();
    if r > echelle { etale * (echelle / r) } else { etale }
}

/// `splay`, la version plate — le témoin. C'est elle qui étalait le patron.
fn splay_plat(pos: Vec2<f64>, map: MapSizeLg) -> Vec2<f64> {
    let echelle = (map.chunks().x as f64) * TerrainChunkSize::RECT_SIZE.x as f64;
    let lod_dist = 2080.0 * 0.95 / echelle;
    let dist = pos.x.abs() + pos.y.abs();
    let etirement = (dist.powf(5.5) * 0.75 + dist * 0.25) * (1.0 - lod_dist) + lod_dist;
    let mut etale = pos * etirement * echelle;
    if pos.x.abs() > 0.99 || pos.y.abs() > 0.99 {
        etale *= 50.0;
    }
    etale
}

/// `cube_exp`, mot pour mot comme dans `include/cube.glsl`.
fn cube_exp(d: Vec2<f64>, haut: Vec3<f64>, est: Vec3<f64>, rayon: f64) -> Vec3<f64> {
    let nord = haut.cross(est);
    let r = d.magnitude();
    if r < 1e-6 {
        return haut;
    }
    let u = d / r;
    let m = est * u.x + nord * u.y;
    let theta = r / rayon;
    haut * theta.cos() + m * theta.sin()
}

/// `cube_face_de_direction` : la face dont la normale domine, puis la
/// gnomonique passée à la table inverse.
fn face_de_direction(d: Vec3<f64>) -> (usize, (f64, f64)) {
    let mut face = 0;
    let mut meilleur = f64::MIN;
    for f in 0..6 {
        let n = cube::BASES[f].n;
        let p = d.x * n[0] as f64 + d.y * n[1] as f64 + d.z * n[2] as f64;
        if p > meilleur {
            meilleur = p;
            face = f;
        }
    }
    let base = cube::BASES[face];
    let proj = |v: [i32; 3]| d.x * v[0] as f64 + d.y * v[1] as f64 + d.z * v[2] as f64;
    let st = common::terrain::conforme::inverse().lire(
        proj(base.r) / meilleur,
        proj(base.h) / meilleur,
    );
    (face, st)
}

/// Le repère orthonormé du foyer, celui que la caméra téléverse.
fn repere_foyer(map: MapSizeLg, wpos: Vec2<f64>) -> (Vec3<f64>, Vec3<f64>) {
    let lieu = cube::lieu_de(map, wpos).expect("le foyer est sur une face");
    let (haut, est, _) = cube::repere_orthonorme(map, lieu);
    (haut, est)
}

// --------------------------------------------------------------------------
// 1 — L'aller-retour de direction, par région
// --------------------------------------------------------------------------

/// **La chaîne du shader retrouve la direction dont elle est partie.**
///
/// Les directions sont tirées **uniformes sur la sphère**, et non uniformes en
/// `(a, b)` : cette dernière loi sur-échantillonne les centres de face et
/// laisserait les coins, qui sont l'endroit difficile, sous la moyenne.
///
/// Le résultat se rend **par région** — centre de face, abord d'arête, abord de
/// coin — parce qu'un maximum global masquerait exactement l'endroit qui échoue.
/// Et il se rend **en blocs**, `R·angle`, parce que c'est l'unité dans laquelle
/// l'erreur se verrait à l'écran.
fn l_aller_retour_de_direction(map: MapSizeLg, rayon: f64) -> u32 {
    let f = cube::face_blocs(map) as f64;
    let chunk = TerrainChunkSize::RECT_SIZE.x as f64;

    let n = 200_000;
    let mut pires = [0.0f64; 3];
    let mut sommes = [0.0f64; 3];
    let mut compte = [0u32; 3];

    for i in 0..n {
        // Spirale de Fibonacci : uniforme en aire, donc uniforme sur la sphère.
        let t = (i as f64 + 0.5) / n as f64;
        let z = 1.0 - 2.0 * t;
        let r = (1.0 - z * z).max(0.0).sqrt();
        let phi = i as f64 * std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
        let d = Vec3::new(r * phi.cos(), r * phi.sin(), z);

        let (face, st) = face_de_direction(d);
        let uv = Vec2::new((st.0 + 1.0) * 0.5 * f, (st.1 + 1.0) * 0.5 * f);
        let lieu = cube::Lieu {
            face: face as u8,
            u: uv.x,
            v: uv.y,
        };
        let retour = cube::direction_de(map, lieu);
        let ecart = rayon * d.dot(retour).clamp(-1.0, 1.0).acos();

        // À quelle distance du bord de sa face ce point tombe-t-il ?
        let bord = (uv.x.min(f - uv.x)).min(uv.y.min(f - uv.y)) / chunk;
        let coin = (uv.x.min(f - uv.x) / chunk).max(uv.y.min(f - uv.y) / chunk);
        let region = if bord < 2.0 && coin < 2.0 {
            2 // les deux bords à la fois : un coin
        } else if bord < 2.0 {
            1
        } else {
            0
        };
        pires[region] = pires[region].max(ecart);
        sommes[region] += ecart;
        compte[region] += 1;
    }

    println!("1 — L'aller-retour de direction, par région (en blocs)");
    let noms = ["centre de face", "abord d'arête", "abord de coin"];
    let mut echecs = 0;
    for i in 0..3 {
        if compte[i] == 0 {
            continue;
        }
        let moyenne = sommes[i] / compte[i] as f64;
        println!(
            "    {:<16} {:>7} points · moyenne {:>8.3} · pire {:>8.3}",
            noms[i], compte[i], moyenne, pires[i]
        );
        // Un texte de carte vaut un chunk : l'erreur doit rester bien en deçà,
        // sans quoi la nappe lirait le mauvais texel.
        if pires[i] > chunk {
            println!("        ÉCHEC : au-delà d'un chunk, la nappe lit le mauvais texel");
            echecs += 1;
        }
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 2 — Aucun sommet sur une case morte
// --------------------------------------------------------------------------

/// **La réfutation directe du défaut d'origine.**
///
/// Le patron laisse dix cases mortes sur seize, soit 62,5 % de la grille. La
/// nappe plate les balayait — c'est le coin noir de la capture. Le disque
/// géodésique ne peut pas les atteindre : la face vient de la normale dominante,
/// donc toujours l'une des six.
///
/// **Le témoin est ce qui rend la mesure opposable :** on rejoue la même grille
/// avec l'ancien `splay`, et l'on exige qu'il tombe bien sur les cases mortes.
/// Un test qui passerait sur les deux versions ne testerait rien.
fn aucun_sommet_sur_une_case_morte(
    map: MapSizeLg,
    rayon: f64,
    portee: f64,
    vd: f64,
    detail: u32,
) -> u32 {
    let f = cube::face_blocs(map) as f64;
    // Un foyer volontairement posé près d'un coin : c'est là que le disque a le
    // plus de chances de mordre sur plusieurs faces à la fois.
    let foyer = Vec2::new(f * 0.97, f * 1.97);
    let (haut, est) = repere_foyer(map, foyer);

    let sommets = grille(detail);

    let mut morts_cube = 0u32;
    let mut morts_plat = 0u32;
    for &pos in &sommets {
        // La chaîne neuve : jamais de case morte, par construction.
        let d = cube_exp(splay_cube(pos, rayon, portee, vd), haut, est, rayon);
        let (face, _) = face_de_direction(d);
        if face >= 6 {
            morts_cube += 1;
        }

        // Le témoin : l'ancienne chaîne, sur la même grille.
        let plat = foyer + splay_plat(pos, map);
        let case_ = plat.map(|e| (e / f).floor() as i32);
        if cube::face_en(case_.x, case_.y).is_none() {
            morts_plat += 1;
        }
    }

    let total = sommets.len() as f64;
    println!("2 — Les sommets de la nappe et les cases mortes du patron");
    println!(
        "    disque géodésique : {morts_cube:>7} morts sur {} ({:.1} %)",
        sommets.len(),
        100.0 * morts_cube as f64 / total
    );
    println!(
        "    témoin, splay plat : {morts_plat:>6} morts sur {} ({:.1} %)",
        sommets.len(),
        100.0 * morts_plat as f64 / total
    );

    let mut echecs = 0;
    if morts_cube != 0 {
        println!("        ÉCHEC : le disque atteint une case morte");
        echecs += 1;
    }
    // Sans témoin qui échoue, le test ne mesure rien.
    if morts_plat * 10 < sommets.len() as u32 {
        println!(
            "        ÉCHEC : le témoin ne tombe presque jamais sur une case morte — la mesure \
             ne distingue plus les deux versions"
        );
        echecs += 1;
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 3 — La couture ne fait pas sauter l'altitude
// --------------------------------------------------------------------------

/// **Le filtrage borné à la face, contre le filtrage qui déborde.**
///
/// L'atlas de LOD range les six faces en croix : deux texels voisins dans
/// l'image ne le sont pas dans le monde. Un filtre bilinéaire qui déborde d'une
/// face mélange donc deux endroits sans rapport.
///
/// **Trois pièges se sont refermés en écrivant cette mesure, et ils valent
/// d'être notés, parce qu'ils rendaient le chiffre inopposable.**
///
/// Le premier : *l'équateur ne prouve rien*. Les quatre faces équatoriales sont
/// rangées côte à côte dans la croix, si bien que le long de l'équateur le
/// voisin du patron **est** le voisin du monde. Le filtrage libre y donne le bon
/// résultat par accident. Il faut un grand cercle qui passe par les calottes,
/// où le patron se trompe pour de bon.
///
/// Le deuxième : *un arc témoin pris à l'intérieur d'une face ne mesure rien
/// non plus*. À un quart de bloc par pas, deux échantillons consécutifs
/// tombent dans le même texel : leur écart mesure la pente de la bilinéaire,
/// pas une discontinuité. Comparer la couture à cela revient à comparer un
/// escalier à une rampe.
///
/// Le troisième, et c'est le seul qui dise quelque chose : **le bon étalon est
/// la résolution de la carte elle-même.** Le LOD ne sait rien de plus fin qu'un
/// chunk ; une marche à la couture qui reste sous l'écart entre deux chunks
/// voisins est indiscernable du crénelage que la nappe porte déjà partout. Au
/// delà, c'est un défaut. C'est donc contre cet écart qu'on juge, et le
/// filtrage libre doit franchement le dépasser — sans quoi la mesure ne
/// distinguerait pas les deux versions.
fn la_couture_ne_fait_pas_sauter_l_altitude(map: MapSizeLg, rayon: f64) -> u32 {
    let fc = cube::face_chunks(map) as usize;
    let cote = fc * 4;
    let fb = cube::face_blocs(map) as f64;

    // Un champ lisse de la direction : aucune discontinuité à trouver, donc
    // toute marche mesurée est un artefact de la lecture, et rien d'autre.
    //
    // **Sa longueur de corrélation est courte, et c'est délibéré.** Un champ à
    // grande échelle donnerait des valeurs voisines même à deux endroits sans
    // rapport : le filtrage libre s'y tromperait sans que le chiffre bouge, et
    // le témoin cesserait de témoigner. Vingt périodes par tour, c'est assez
    // court pour que deux faces soient décorrélées, et assez long pour qu'un
    // chunk reste un petit pas — sans quoi l'étalon lui-même perdrait son sens.
    const K: f64 = 20.0;
    let champ = |d: Vec3<f64>| {
        0.5 + 0.2 * (K * d.x).sin() * (K * d.y).cos() + 0.2 * (K * d.z).sin()
    };

    let centre = |face: usize, i: usize, j: usize| {
        let u = (i as f64 + 0.5) / fc as f64 * fb;
        let v = (j as f64 + 0.5) / fc as f64 * fb;
        cube::direction_de(map, cube::Lieu {
            face: face as u8,
            u,
            v,
        })
    };

    let mut atlas = vec![0.0f64; cote * cote];
    // L'étalon : l'écart le plus grand entre deux chunks voisins de l'atlas.
    // C'est la plus fine variation que la carte de LOD sache représenter.
    let mut pas_de_chunk = 0.0f64;
    for face in 0..6usize {
        let (col, ligne) = cube::PATRON[face];
        for j in 0..fc {
            for i in 0..fc {
                let v = champ(centre(face, i, j));
                atlas[(ligne as usize * fc + j) * cote + col as usize * fc + i] = v;
                if i + 1 < fc {
                    pas_de_chunk = pas_de_chunk.max((champ(centre(face, i + 1, j)) - v).abs());
                }
                if j + 1 < fc {
                    pas_de_chunk = pas_de_chunk.max((champ(centre(face, i, j + 1)) - v).abs());
                }
            }
        }
    }

    // La lecture bornée : celle de `cube_atlas_alt16`.
    let lire_borne = |face: usize, st: (f64, f64)| -> f64 {
        let (col, ligne) = cube::PATRON[face];
        let fcf = fc as f64;
        let px = (st.0 + 1.0) * 0.5 * fcf - 0.5;
        let py = (st.1 + 1.0) * 0.5 * fcf - 0.5;
        let (x0, y0) = (px.floor(), py.floor());
        let (fx, fy) = (px - x0, py - y0);
        let t = |dx: f64, dy: f64| {
            let x = (x0 + dx).clamp(0.0, fcf - 1.0) as usize + col as usize * fc;
            let y = (y0 + dy).clamp(0.0, fcf - 1.0) as usize + ligne as usize * fc;
            atlas[y * cote + x]
        };
        let bas = t(0.0, 0.0) * (1.0 - fx) + t(1.0, 0.0) * fx;
        let hau = t(0.0, 1.0) * (1.0 - fx) + t(1.0, 1.0) * fx;
        bas * (1.0 - fy) + hau * fy
    };

    // Le témoin : la même bilinéaire, libre de sortir de la face — ce que fait
    // un échantillonneur matériel lâché sur l'atlas entier.
    let lire_libre = |face: usize, st: (f64, f64)| -> f64 {
        let (col, ligne) = cube::PATRON[face];
        let fcf = fc as f64;
        let px = (st.0 + 1.0) * 0.5 * fcf - 0.5 + col as f64 * fcf;
        let py = (st.1 + 1.0) * 0.5 * fcf - 0.5 + ligne as f64 * fcf;
        let (x0, y0) = (px.floor(), py.floor());
        let (fx, fy) = (px - x0, py - y0);
        let t = |dx: f64, dy: f64| {
            let x = (x0 + dx).clamp(0.0, cote as f64 - 1.0) as usize;
            let y = (y0 + dy).clamp(0.0, cote as f64 - 1.0) as usize;
            atlas[y * cote + x]
        };
        let bas = t(0.0, 0.0) * (1.0 - fx) + t(1.0, 0.0) * fx;
        let hau = t(0.0, 1.0) * (1.0 - fx) + t(1.0, 1.0) * fx;
        bas * (1.0 - fy) + hau * fy
    };

    // On ne mesure pas un écart d'un échantillon au suivant : une lecture
    // fausse peut dériver **graduellement** sur un demi-texel, et le pas à pas
    // ne la voit pas. On mesure l'**erreur vraie**, contre le champ dont on
    // connaît la formule — c'est la seule quantité qui dise si la lecture ment.
    //
    // Et on la range par région : à l'intérieur d'une face, la lecture ne peut
    // se tromper que de sa quantification ; c'est l'étalon contre lequel juger
    // ce qui se passe aux coutures.
    let fbf = fb;
    let chunk = TerrainChunkSize::RECT_SIZE.x as f64;
    let parcourir = |axe_x: Vec3<f64>, axe_y: Vec3<f64>| {
        let pas = 0.25 / rayon;
        let n = (std::f64::consts::TAU / pas) as usize;
        // [intérieur, abord de couture] × [bornée, libre]
        let mut pire = [[0.0f64; 2]; 2];
        for k in 0..=n {
            let a = k as f64 * pas;
            let d = (axe_x * a.cos() + axe_y * a.sin()).normalized();
            let (face, st) = face_de_direction(d);
            let verite = champ(d);

            let uv = Vec2::new((st.0 + 1.0) * 0.5 * fbf, (st.1 + 1.0) * 0.5 * fbf);
            let au_bord = (uv.x.min(fbf - uv.x)).min(uv.y.min(fbf - uv.y)) / chunk;
            let region = if au_bord <= 1.0 {
                1
            } else if au_bord >= 3.0 {
                0
            } else {
                continue; // la zone grise ne dit rien de tranché
            };

            pire[region][0] = pire[region][0].max((lire_borne(face, st) - verite).abs());
            pire[region][1] = pire[region][1].max((lire_libre(face, st) - verite).abs());
        }
        pire
    };

    // Un méridien : il traverse les deux calottes, donc des coutures que la
    // croix ne rend pas voisines. C'est là que le patron se trompe.
    let m = parcourir(Vec3::unit_x(), Vec3::unit_z());

    println!("3 — L'erreur de lecture de l'atlas, selon qu'on est ou non sur une couture");
    println!("    étalon : le plus grand écart entre deux chunks voisins = {pas_de_chunk:.5}");
    println!(
        "    bornée à la face : intérieur {:.5} · abord de couture {:.5}",
        m[0][0], m[1][0]
    );
    println!(
        "    libre (témoin)   : intérieur {:.5} · abord de couture {:.5}",
        m[0][1], m[1][1]
    );

    let mut echecs = 0;
    // La borne attendue n'est pas choisie. Au bord d'une face, la lecture bornée
    // remplace une interpolation par un maintien sur un demi-texel : elle se
    // trompe donc au plus de ce que le champ varie sur un chunk. L'étalon *est*
    // la borne.
    if m[1][0] > pas_de_chunk {
        println!(
            "        ÉCHEC : bornée, l'erreur à la couture ({:.5}) dépasse la résolution de la \
             carte",
            m[1][0]
        );
        echecs += 1;
    }
    // Et si le témoin ne se trompait pas davantage, c'est que le parcours ne
    // passe par aucune couture mal rangée : la mesure ne distinguerait pas les
    // deux versions, et le chiffre ci-dessus ne serait qu'un alibi.
    if m[1][1] < pas_de_chunk * 3.0 {
        println!(
            "        ÉCHEC : libre, l'erreur à la couture ({:.5}) reste dans la résolution de la \
             carte — le parcours ne traverse pas de couture mal rangée",
            m[1][1]
        );
        echecs += 1;
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 4 — Le rayon de la nappe ne traverse jamais le centre
// --------------------------------------------------------------------------

/// **La réfutation du défaut le plus cher de cette passe.**
///
/// `pull_down` cache la nappe sous les chunks chargés en l'enfonçant. Son
/// expression, `1 / (portée / distance de vue)^20`, explose en deçà de la
/// distance de vue : à cinquante blocs du joueur elle vaut 4,8·10¹⁵.
///
/// Sur une carte plate ce nombre enfonce en −Z, hors du champ, et le triangle
/// est clippé. Sur une sphère il rend le **rayon négatif**, ce qui retourne la
/// direction : le sommet repart par l'antipode, et le triangle qui le relie à
/// son voisin traverse tout l'univers en passant devant la caméra.
///
/// **Le témoin est la même chaîne sans la borne**, et il doit échouer
/// franchement — sinon la borne ne sert à rien et le chiffre est un alibi.
fn le_rayon_de_la_nappe_reste_positif(
    map: MapSizeLg,
    rayon: f64,
    portee_max: f64,
    detail: u32,
) -> u32 {
    // Le repère n'intervient pas : `pull_down` ne dépend que de la portée, donc
    // du seul `splay_cube`.
    let sommets = grille(detail);

    // Une altitude de terrain plausible : ce n'est pas elle qui décide.
    let alt = 200.0;

    let mut pire_borne = f64::INFINITY;
    let mut negatifs_libres = 0u32;
    let mut pire_libre = f64::INFINITY;

    // Plusieurs distances de vue : c'est elle qui déplace le point d'explosion.
    for vd in [160.0, 320.0, 1000.0, 2080.0] {
        for &pos in &sommets {
            let d = splay_cube(pos, rayon, portee_max, vd);
            let portee = d.magnitude();
            let brut = 1.0 / (portee / (vd * 0.95)).powi(20);

            // La version bornée, celle du shader.
            let borne = brut.min(rayon * 0.5);
            pire_borne = pire_borne.min(rayon + alt - borne - 0.1);

            // Le témoin : sans borne.
            let libre = rayon + alt - brut - 0.1;
            pire_libre = pire_libre.min(libre);
            if libre <= 0.0 {
                negatifs_libres += 1;
            }
        }
    }

    let total = sommets.len() as u32 * 4;
    println!("4 — Le rayon de la nappe, sous le retrait `pull_down`");
    println!("    borné  : rayon minimal {pire_borne:>12.1} bloc sur {total} sommets");
    println!(
        "    témoin : rayon minimal {pire_libre:>12.3e} · {negatifs_libres} sommets derrière le \
         centre ({:.1} %)",
        100.0 * negatifs_libres as f64 / total as f64
    );

    let mut echecs = 0;
    if pire_borne <= 0.0 {
        println!("        ÉCHEC : la nappe traverse encore le centre de la planète");
        echecs += 1;
    }
    if negatifs_libres == 0 {
        println!(
            "        ÉCHEC : le témoin ne traverse jamais le centre — la borne ne corrige rien, \
             et la mesure ne prouve rien"
        );
        echecs += 1;
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 5 — La calotte suit l'observateur
// --------------------------------------------------------------------------

/// **Le lointain se taille sur la caméra, pas sur le joueur.**
///
/// Le foyer reste au sol pendant que la caméra recule. Taillée sur l'altitude
/// du foyer, la calotte gardait la portée d'un observateur au sol, et la
/// planète apparaissait tronquée dès qu'on prenait du recul — c'est le fond
/// manquant de la vue orbitale.
///
/// Deux exigences : la calotte **couvre toujours** l'horizon géométrique de
/// l'observateur, et elle **croît** avec son altitude. Le témoin est la version
/// qui lit l'altitude du foyer, dont la portée ne bouge pas d'un iota.
fn la_calotte_suit_l_observateur(map: MapSizeLg, rayon: f64) -> u32 {
    let h_max = 2188.0; // niveau de la mer + hauteur maximale, carte par défaut
    let h_foyer = 200.0;
    let vd = 2080.0;

    println!("5 — La portée du lointain, selon l'altitude de la caméra");
    println!(
        "    {:>10}  {:>12}  {:>12}  {:>12}",
        "h caméra", "horizon", "calotte", "témoin (foyer)"
    );

    let mut echecs = 0;
    let mut precedent = 0.0f64;
    let temoin = cube::horizon(map, h_foyer, h_max)
        .max(1.05 * vd / rayon)
        .min(2.8);

    for h_cam in [0.0, 50.0, 200.0, 2_000.0, 20_000.0, 1e6] {
        let theta = cube::horizon(map, h_cam, h_max)
            .max(1.05 * vd / rayon)
            .min(2.8);
        // L'horizon géométrique nu, sans le rattrapage des sommets.
        let horizon = rayon * cube::horizon(map, h_cam, 0.0);
        println!(
            "    {h_cam:>10.0}  {horizon:>12.0}  {:>12.0}  {:>12.0}",
            rayon * theta,
            rayon * temoin
        );

        if !theta.is_finite() || theta <= 0.0 {
            println!("        ÉCHEC : portée dégénérée à h = {h_cam}");
            echecs += 1;
        }
        if rayon * theta < horizon {
            println!("        ÉCHEC : la calotte s'arrête en deçà de l'horizon à h = {h_cam}");
            echecs += 1;
        }
        if theta < precedent {
            println!("        ÉCHEC : la portée décroît quand la caméra monte");
            echecs += 1;
        }
        precedent = theta;
    }

    // Et le témoin doit être franchement dépassé de loin, sinon la correction
    // ne change rien.
    let haut = cube::horizon(map, 1e6, h_max).min(2.8);
    if haut < temoin * 2.0 {
        println!(
            "        ÉCHEC : depuis l'orbite la calotte ne dépasse pas le double de celle du \
             foyer — la mesure ne distingue pas les deux versions"
        );
        echecs += 1;
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 6 — Le pont de nuages est une coquille
// --------------------------------------------------------------------------

/// **La dalle plate est devenue une coquille, et le chiffre le dit.**
///
/// Le pont de nuages était une intersection rayon-plan contre `z = cloud_alt`.
/// Vue de haut c'est un disque infini sans rapport avec la planète — le grand
/// voile qui barrait le ciel depuis l'orbite. On mesure donc, pour un faisceau
/// de directions, la **hauteur au-dessus de la sphère** du point d'intersection :
/// elle doit être constante à l'arrondi près.
///
/// Le témoin est l'intersection rayon-plan sur les mêmes directions, dont la
/// hauteur dérive de milliers de blocs dès qu'on quitte le zénith.
fn le_pont_de_nuages_est_une_coquille(rayon: f64) -> u32 {
    let cloud_alt = 2000.0;
    let rc = rayon + cloud_alt;
    // Un observateur à cinq cents blocs, comme un joueur en aéronef.
    let origine = Vec3::new(0.0, 0.0, rayon + 500.0);

    let mut ecarts_coquille: Vec<f64> = Vec::new();
    let mut ecarts_plan: Vec<f64> = Vec::new();

    for i in 0..1000 {
        // Des directions réparties du zénith jusqu'à raser l'horizon.
        let t = (i as f64 + 0.5) / 1000.0;
        let theta = t * 1.4; // jusqu'à 80° du zénith
        let phi = i as f64 * 2.399963;
        let dir = Vec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos());

        // La coquille : intersection rayon-sphère de rayon `R + cloud_alt`.
        let b = origine.dot(dir);
        let disc = b * b - (origine.dot(origine) - rc * rc);
        if disc <= 0.0 {
            continue;
        }
        let d = -b + disc.sqrt();
        let p = origine + dir * d;
        ecarts_coquille.push((p.magnitude() - rayon - cloud_alt).abs());

        // Le témoin : le plan `z = R + cloud_alt`, celui de la version plate.
        if dir.z > 1e-6 {
            let dp = (rc - origine.z) / dir.z;
            let q = origine + dir * dp;
            ecarts_plan.push((q.magnitude() - rayon - cloud_alt).abs());
        }
    }

    let pire = |v: &Vec<f64>| v.iter().cloned().fold(0.0f64, f64::max);
    let pire_coquille = pire(&ecarts_coquille);
    let pire_plan = pire(&ecarts_plan);

    println!("6 — La hauteur du pont de nuages au-dessus de la sphère");
    println!(
        "    coquille : écart maximal {pire_coquille:.6} bloc sur {} directions",
        ecarts_coquille.len()
    );
    println!(
        "    témoin (plan) : écart maximal {pire_plan:.0} bloc sur {} directions",
        ecarts_plan.len()
    );

    let mut echecs = 0;
    if pire_coquille > 1e-6 {
        println!("        ÉCHEC : la coquille n'est pas à hauteur constante");
        echecs += 1;
    }
    if pire_plan < 1000.0 {
        println!(
            "        ÉCHEC : le plan ne dérive pas — le faisceau ne s'écarte pas assez du zénith, \
             la mesure ne distingue pas les deux versions"
        );
        echecs += 1;
    }
    println!();
    echecs
}

// --------------------------------------------------------------------------
// 7 — Le champ de repères est continu
// --------------------------------------------------------------------------

/// **Ce qui décide si la marche voxel scintille.**
///
/// La marche pose son réseau dans les coordonnées normales géodésiques rendues
/// par `cube_log`. Pour que deux fragments voisins tombent dans la même case, le
/// champ de repères doit varier continûment ; un repère bâti au hasard
/// perpendiculairement à la verticale saute, et le réseau avec lui.
///
/// On marche le long d'un grand cercle et on mesure la rotation du repère d'un
/// échantillon au suivant. Le témoin est le repère naïf, `cross(haut, axe fixe)`,
/// qui doit s'affoler au voisinage du pôle de cet axe.
fn le_champ_de_reperes_est_continu(rayon: f64) -> u32 {
    let haut = Vec3::unit_z();
    let est = Vec3::unit_x();

    let pas = 0.25 / rayon;
    let n = (1.9 / pas) as usize; // au-delà de π/2 : on traverse le pôle du témoin

    let mut pire_log = 0.0f64;
    let mut pire_naif = 0.0f64;
    let mut precedent: Option<(Vec3<f64>, Vec3<f64>)> = None;

    for k in 0..=n {
        // **L'arc doit passer par le pôle du repère naïf, sinon le témoin ne
        // témoigne pas.** `cross(unit_x, dir)` dégénère quand `dir` approche
        // `±X` : on balaie donc du zénith jusqu'à `+X` et au-delà. Un arc qui
        // éviterait ce point laisserait le repère naïf tranquille, et la mesure
        // passerait sur les deux versions sans rien distinguer — le piège dans
        // lequel la mesure de la couture est tombée trois fois.
        let a = k as f64 * pas;
        let dir = (haut * a.cos() + Vec3::unit_x() * a.sin()).normalized();

        // Le repère de `cube_log` puis `cube_exp`.
        let d = cube_log(dir, haut, est, rayon);
        let (e1, _n1) = cube_exp_repere(d, haut, est, rayon);

        // Le témoin : perpendiculaire à la verticale, tirée d'un axe fixe.
        let e2 = {
            let c = Vec3::unit_x().cross(dir);
            if c.magnitude() < 1e-9 {
                Vec3::unit_y()
            } else {
                c.normalized()
            }
        };

        if let Some((p1, p2)) = precedent {
            pire_log = pire_log.max(p1.dot(e1).clamp(-1.0, 1.0).acos().to_degrees());
            pire_naif = pire_naif.max(p2.dot(e2).clamp(-1.0, 1.0).acos().to_degrees());
        }
        precedent = Some((e1, e2));
    }

    println!("7 — La rotation du repère d'un quart de bloc au suivant");
    println!("    par `cube_log` : {pire_log:.9}°");
    println!("    témoin naïf    : {pire_naif:.3}°");

    let mut echecs = 0;
    // Un quart de bloc sur un rayon de 5 036 vaut 0,0028° d'arc : le repère ne
    // doit pas tourner plus vite que le point ne se déplace.
    if pire_log > 0.01 {
        println!("        ÉCHEC : le repère de `cube_log` saute — le réseau scintillerait");
        echecs += 1;
    }
    if pire_naif < 1.0 {
        println!(
            "        ÉCHEC : le témoin naïf tourne de moins d'un degré — l'arc ne passe pas par \
             le pôle de son axe, la mesure ne prouve rien"
        );
        echecs += 1;
    }
    println!();
    echecs
}

/// `cube_log`, mot pour mot comme dans `include/cube.glsl`.
fn cube_log(dir: Vec3<f64>, haut: Vec3<f64>, est: Vec3<f64>, rayon: f64) -> Vec2<f64> {
    let nord = haut.cross(est);
    let c = haut.dot(dir).clamp(-1.0, 1.0);
    let m = dir - haut * c;
    let l = m.magnitude();
    if l < 1e-7 {
        return Vec2::zero();
    }
    let m = m / l;
    let theta = c.acos();
    Vec2::new(m.dot(est), m.dot(nord)) * (theta * rayon)
}

/// Le repère analytique que `cube_exp` rend en sortie.
fn cube_exp_repere(
    d: Vec2<f64>,
    haut: Vec3<f64>,
    est: Vec3<f64>,
    rayon: f64,
) -> (Vec3<f64>, Vec3<f64>) {
    let nord = haut.cross(est);
    let r = d.magnitude();
    if r < 1e-6 {
        return (est, nord);
    }
    let u = d / r;
    let m = est * u.x + nord * u.y;
    let theta = r / rayon;
    let dm = -haut * theta.sin() + m * theta.cos();
    let perp = -est * u.y + nord * u.x;
    (dm * u.x - perp * u.y, dm * u.y + perp * u.x)
}

/// La grille de `create_lod_terrain_mesh`, en coordonnées de paramètres.
fn grille(detail: u32) -> Vec<Vec2<f64>> {
    let detail = detail + 1;
    Spiral2d::new()
        .take((detail * detail) as usize)
        .skip(1)
        .map(|p| {
            let x = p.x + detail as i32 / 2;
            let y = p.y + detail as i32 / 2;
            Vec2::new(x, y).map(|e| 2.0 * e as f64 / detail as f64 - 1.0)
        })
        .collect()
}

// --------------------------------------------------------------------------
// 8 — Le profil de la nappe sous le joueur
// --------------------------------------------------------------------------

/// **La fosse était à l'écran et absente des chiffres.**
///
/// `pull_down` enfonce la nappe pour la cacher sous les chunks chargés. Sur une
/// carte plate il vaut 10¹⁵ : la géométrie sort du tronc de vue et se fait
/// *supprimer*. Sur une sphère, **enfoncer ne supprime pas, ça creuse** — la
/// fosse reste finie, dessinée, et dans le champ.
///
/// **La première version de cette mesure jugeait la pente de la paroi, et
/// c'était le mauvais critère.** La paroi est enterrée sous le terrain chargé
/// quelle que soit sa raideur : elle n'est jamais vue de face. Et la pente ne
/// dit rien de plus que la profondeur, puisque la courbe en puissance vingt
/// gagne la moitié de sa profondeur sur une cinquantaine de blocs dans les deux
/// versions — seule l'échelle verticale change.
///
/// Ce qui décide est la **profondeur**, et elle est prise entre deux bornes :
/// assez grande pour que la nappe reste sous le relief d'un chunk — sinon elle
/// perce le sol —, assez petite pour qu'un trou de chargement montre une chute
/// de terrain plutôt qu'un gouffre. Le témoin, l'ancienne borne `R/2`, doit
/// dépasser franchement la seconde.
fn le_profil_de_la_nappe(rayon: f64, vd: f64) -> u32 {
    let bord = vd * 0.95;

    // Profondeur, rayon du fond plat, et largeur sur laquelle la fosse gagne la
    // moitié de sa profondeur.
    let profil = |plafond: f64| {
        let fond = bord * (1.0 / plafond).powf(0.05);
        let moitie = bord * (1.0 / (plafond * 0.5)).powf(0.05);
        (plafond, fond, moitie - fond)
    };

    let (prof, fond, creuse) = profil(300.0);
    let (t_prof, t_fond, t_creuse) = profil(rayon * 0.5);

    // Le relief à l'intérieur d'un chunk : ce que la nappe doit passer sous le
    // terrain réel pour ne pas le percer.
    const RELIEF_DE_CHUNK: f64 = 150.0;
    // Au-delà, ce qu'on aperçoit par un trou de chargement cesse de ressembler
    // à du terrain.
    const GOUFFRE: f64 = 500.0;

    println!("8 — Le profil de l'enfoncement de la nappe autour du joueur");
    println!("    retenu : fond à {prof:.0} blocs, plat jusqu'à {fond:.0} de portée");
    println!("             la fosse gagne la moitié de sa profondeur sur {creuse:.0} blocs");
    println!("    témoin (borne R/2) : fond à {t_prof:.0} blocs, plat jusqu'à {t_fond:.0}");
    println!("             et gagne la moitié de la sienne sur {t_creuse:.0} blocs");
    println!("    bornes admises : entre {RELIEF_DE_CHUNK:.0} et {GOUFFRE:.0} blocs");

    let mut echecs = 0;
    if prof < RELIEF_DE_CHUNK {
        println!("        ÉCHEC : trop peu enfoncée, la nappe percerait le terrain réel");
        echecs += 1;
    }
    if prof > GOUFFRE {
        println!("        ÉCHEC : trop enfoncée, un trou de chargement ouvrirait un gouffre");
        echecs += 1;
    }
    if t_prof <= GOUFFRE {
        println!(
            "        ÉCHEC : le témoin ne creuse pas de gouffre — la mesure ne distingue rien"
        );
        echecs += 1;
    }
    println!();
    echecs
}
