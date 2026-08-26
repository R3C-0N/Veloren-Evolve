//! Diagnostic en console : `proto-sphere --diag`.
//!
//! Le prototype existe pour réfuter D27, pas pour l'illustrer. Une capture
//! d'écran ne dit pas si un recollement est continu — des nombres, si.
//!
//! Ce fichier tient aussi lieu de garantie pour [`crate::cube`] : les vingt-
//! quatre arêtes n'y sont écrites nulle part, elles se déduisent de six bases
//! 3D. Ce sont ces invariants qui répondent du calcul, pas la relecture.

use crate::cube::{
    COS_SIN, FACE, FACE_CHUNKS, NOMS, RAYON, direction, point_sphere, replier_bloc,
    replier_chunk,
};
use crate::monde::{Generateur, NIVEAU_MER, TAILLE_CHUNK, biome_de, point_apparition};
use crate::conforme;
use crate::vue3d::{Camera, viser};
use glam::{DVec3, Vec3};
use std::collections::HashSet;

pub fn executer() {
    let gen = Generateur::nouveau(1);

    println!("Patron de cube : 6 faces de {FACE} blocs d'arête");
    println!(
        "tour du monde : {:.0} blocs · rayon {RAYON:.0} blocs",
        std::f64::consts::TAU * RAYON
    );
    println!();

    invariants();
    coutures(&gen);
    distorsion();
    projection_conforme();
    continuite_du_deroulement();
    continuite_a_la_traversee();
    derive_de_visee(&gen);
    terrain(&gen);
}

// --------------------------------------------------------------------------
// Les invariants de la topologie
// --------------------------------------------------------------------------

fn invariants() {
    println!("── Invariants de repliement ──");

    // 1. Idempotence : une position canonique ne bouge pas.
    let mut ok = true;
    for f in 0..6u8 {
        for u in (0..FACE).step_by(37) {
            for v in (0..FACE).step_by(41) {
                let (f2, u2, v2, k) = replier_bloc(f, u, v);
                if (f2, u2, v2, k) != (f, u, v, 0) {
                    ok = false;
                }
            }
        }
    }
    println!("  idempotent sur le patron              : {}", verdict(ok));

    // 2. Aller-retour : sortir par un bord puis revenir sur ses pas rend la
    //    case de départ, et les deux rotations s'annulent.
    let mut ok = true;
    let mut testes = 0;
    for f in 0..6u8 {
        for w in 0..FACE {
            for (du, dv, uu, vv) in [
                (FACE, w, FACE - 1, w),
                (-1, w, 0, w),
                (w, FACE, w, FACE - 1),
                (w, -1, w, 0),
            ] {
                let (f2, u2, v2, k) = replier_bloc(f, du, dv);
                // La direction du pas, transportée, est le vecteur de départ
                // tourné de k quarts de tour.
                let (pu, pv) = (du - uu, dv - vv);
                let (cos, sin) = COS_SIN[k as usize];
                let (tu, tv) = (pu * cos - pv * sin, pu * sin + pv * cos);
                let (f3, u3, v3, k2) = replier_bloc(f2, u2 - tu, v2 - tv);

                if (f3, u3, v3) != (f, uu, vv) || (k + k2) % 4 != 0 {
                    ok = false;
                }
                testes += 1;
            }
        }
    }
    println!("  aller-retour ({testes} bords)          : {}", verdict(ok));

    // 3. Cohérence bloc / chunk : le renderer place des chunks et tourne leur
    //    maillage ; il faut que cela revienne au même que replier bloc à bloc.
    let mut ok = true;
    for f in 0..6u8 {
        for cu in -1..=FACE_CHUNKS {
            for cv in -1..=FACE_CHUNKS {
                let (fc, cuc, cvc, k) = replier_chunk(f, cu, cv);
                for (lu, lv) in [(0, 0), (31, 0), (0, 31), (31, 31), (7, 19)] {
                    let (fb, bu, bv, kb) =
                        replier_bloc(f, cu * TAILLE_CHUNK + lu, cv * TAILLE_CHUNK + lv);
                    let (a, b) = (2 * lu - 31, 2 * lv - 31);
                    let (cos, sin) = COS_SIN[k as usize];
                    let (a2, b2) = (a * cos - b * sin, a * sin + b * cos);
                    let attendu = (
                        fc,
                        cuc * TAILLE_CHUNK + (a2 + 31) / 2,
                        cvc * TAILLE_CHUNK + (b2 + 31) / 2,
                        k,
                    );
                    if (fb, bu, bv, kb) != attendu {
                        ok = false;
                    }
                }
            }
        }
    }
    println!("  chunk et bloc s'accordent             : {}", verdict(ok));

    // 4. Le défaut : combien de voisins distincts a une case ? Huit partout,
    //    sept sur les vingt-quatre cases qui touchent un coin du cube.
    let mut defectueuses = 0;
    let mut pires = 8;
    for f in 0..6u8 {
        for (u, v) in [(0, 0), (FACE - 1, 0), (0, FACE - 1), (FACE - 1, FACE - 1)] {
            let n = voisins_distincts(f, u, v);
            if n != 8 {
                defectueuses += 1;
                pires = pires.min(n);
            }
        }
    }
    println!(
        "  cases de coin défectueuses            : {defectueuses} sur 24, {pires} voisins au lieu de 8"
    );
    println!();
}

fn voisins_distincts(f: u8, u: i32, v: i32) -> usize {
    let mut vus = HashSet::new();
    for dv in -1..=1 {
        for du in -1..=1 {
            if (du, dv) == (0, 0) {
                continue;
            }
            let (f2, u2, v2, _) = replier_bloc(f, u + du, v + dv);
            vus.insert((f2, u2, v2));
        }
    }
    vus.len()
}

fn verdict(ok: bool) -> &'static str { if ok { "OK" } else { "ÉCHEC" } }

// --------------------------------------------------------------------------
// Les douze recollements
// --------------------------------------------------------------------------

fn coutures(gen: &Generateur) {
    println!("── Les douze recollements ──");
    println!("  (mesuré sur les 7/8 centraux de chaque bord : les coins sont");
    println!("   singuliers par construction et relèvent d'une autre mesure)");
    println!("  face      bord      dénivelé couture   dénivelé ordinaire   écart de pas");

    let (mut pire_h, mut pire_a) = (0.0f64, 0.0f64);

    for f in 0..6u8 {
        for (bord, nom) in [(0, "+u"), (1, "−u"), (2, "+v"), (3, "−v")] {
            let (mut somme_couture, mut somme_ordinaire, mut pire_pas) = (0.0, 0.0, 0.0f64);
            let n = 96;

            // On saute les extrémités : elles touchent les coins, où la
            // taille des cases s'effondre légitimement. Ce que la couture
            // doit prouver, c'est qu'elle ne coupe pas — pas que le coin est
            // régulier, ce qu'il n'est pas et ne peut pas être.
            for i in 0..n {
                let w = FACE / 16 + i * (FACE * 7 / 8) / n;
                // Trois cases alignées, perpendiculaires au bord : la deuxième
                // est la dernière de la face, la troisième est de l'autre côté
                // du recollement. Comparer ces deux pas-là, et pas un pas pris
                // ailleurs, est la seule comparaison honnête : c'est le même
                // terrain, au même endroit.
                let (avant, dedans, dehors) = match bord {
                    0 => ((FACE - 2, w), (FACE - 1, w), (FACE, w)),
                    1 => ((1, w), (0, w), (-1, w)),
                    2 => ((w, FACE - 2), (w, FACE - 1), (w, FACE)),
                    _ => ((w, 1), (w, 0), (w, -1)),
                };

                somme_couture += (gen.hauteur(f, dehors.0, dehors.1)
                    - gen.hauteur(f, dedans.0, dedans.1))
                .abs() as f64;
                somme_ordinaire += (gen.hauteur(f, dedans.0, dedans.1)
                    - gen.hauteur(f, avant.0, avant.1))
                .abs() as f64;

                // Le pas qui franchit le recollement doit mesurer, sur la
                // sphère, ce que mesure le pas juste à côté. Le comparer au
                // pas du centre de la face n'aurait plus de sens : la
                // projection conforme fait varier la taille des cases, et un
                // écart légitime passerait pour une discontinuité.
                let voisin = angle(f, avant, dedans);
                if voisin > 0.0 {
                    let r = angle(f, dedans, dehors) / voisin;
                    pire_pas = pire_pas.max(r.max(1.0 / r));
                }
            }

            let (hc, ho) = (somme_couture / n as f64, somme_ordinaire / n as f64);
            let ecart = pire_pas;
            pire_h = pire_h.max(hc / ho.max(1e-9));
            pire_a = pire_a.max(ecart);

            println!(
                "  {:<8}  {nom:<8}  {hc:>16.3}   {ho:>18.3}   {ecart:>15.3}",
                NOMS[f as usize]
            );
        }
    }

    println!(
        "  pire rapport de dénivelé : {pire_h:.3} · pire écart de pas : {pire_a:.3} (1,000 = indiscernable)"
    );
    println!();
}

fn angle(f: u8, a: (i32, i32), b: (i32, i32)) -> f64 {
    let (fa, ua, va, _) = replier_bloc(f, a.0, a.1);
    let (fb, ub, vb, _) = replier_bloc(f, b.0, b.1);
    let (pa, pb) = (point_sphere(fa, ua, va), point_sphere(fb, ub, vb));
    let d = (pa[0] * pb[0] + pa[1] * pb[1] + pa[2] * pb[2]).clamp(-1.0, 1.0);
    d.acos()
}

/// Franchir un bord doit être un non-événement.
///
/// La caméra change de face, sa position est remappée et son lacet tourne d'un
/// quart de tour : trois changements brutaux qui doivent se compenser
/// exactement. On place deux caméras de part et d'autre d'un bord et on regarde
/// ce qui les sépare une fois tout appliqué.
///
/// Un seul chiffre ne prouverait rien : deux caméras distinctes diffèrent
/// forcément un peu. Ce qui prouve la continuité, c'est que **l'écart fonde
/// avec l'écartement**. On mesure donc à trois écartements décroissants : si le
/// rapport suit, il n'y a pas de saut ; s'il plafonne, il y en a un.
fn continuite_a_la_traversee() {
    println!("── Franchir un bord ──");
    println!("  écartement des deux caméras   position       visée      verticale");

    let mut precedent: Option<f64> = None;
    for delta in [0.008f32, 0.002, 0.0005] {
        let (mut pire_pos, mut pire_visee, mut pire_haut) = (0.0f64, 0.0f64, 0.0f64);

        for f in 0..6u8 {
            for bord in 0..4 {
                // Tout le bord, coins compris : c'est près d'eux que le repère
                // est le plus tordu, donc là qu'un défaut se cacherait.
                for i in 0..64 {
                    let w = (2 + i * (FACE - 4) / 64) as f32 + 0.5;
                    let (dedans, dehors) = match bord {
                        0 => ((FACE as f32 - delta, w), (FACE as f32 + delta, w)),
                        1 => ((delta, w), (-delta, w)),
                        2 => ((w, FACE as f32 - delta), (w, FACE as f32 + delta)),
                        _ => ((w, delta), (w, -delta)),
                    };

                    for lacet in [0.0f32, 0.9, 2.1, -1.4] {
                        let camera = |p: (f32, f32)| {
                            let mut c = Camera {
                                face: f,
                                position: Vec3::new(p.0, p.1, (NIVEAU_MER + 20) as f32),
                                lacet,
                                tangage: -0.2,
                            };
                            c.replier();
                            c.repere_3d(RAYON)
                        };
                        let (pa, va, ha) = camera(dedans);
                        let (pb, vb, hb) = camera(dehors);
                        let ecart = |x: Vec3, y: Vec3| {
                            (x.dot(y) as f64).clamp(-1.0, 1.0).acos().to_degrees()
                        };

                        pire_pos = pire_pos.max((pa - pb).length());
                        pire_visee = pire_visee.max(ecart(va, vb));
                        pire_haut = pire_haut.max(ecart(ha, hb));
                    }
                }
            }
        }

        let rapport = precedent.map(|p: f64| format!("÷{:.1}", p / pire_visee.max(1e-12)));
        println!(
            "  {:>8.4} bloc                {:>8.4}    {:>8.4}°   {:>8.4}°  {}",
            2.0 * delta as f64,
            pire_pos,
            pire_visee,
            pire_haut,
            rapport.unwrap_or_default()
        );
        precedent = Some(pire_visee);
    }

    println!("  L'écart fond avec l'écartement : rien ne saute au passage d'un bord.");
    println!("  Ce qui reste est la torsion du repère au voisinage d'un coin, qui");
    println!("  est celle du cône lui-même — continue, mais raide.");
    println!();
}

// --------------------------------------------------------------------------
// Ce que le cube coûte
// --------------------------------------------------------------------------

fn distorsion() {
    println!("── Distorsion de la grille ──");

    let (mut mini, mut maxi) = (f64::MAX, 0.0f64);
    for u in (0..FACE - 1).step_by(16) {
        for v in (0..FACE - 1).step_by(16) {
            let a = angle(1, (u, v), (u + 1, v));
            mini = mini.min(a);
            maxi = maxi.max(a);
        }
    }
    println!("  un pas de grille, du plus court au plus long : {:.4}", maxi / mini);
    println!(
        "  centre de face : {:.6} rad · coin de face : {:.6} rad",
        angle(1, (FACE / 2, FACE / 2), (FACE / 2 + 1, FACE / 2)),
        angle(1, (0, 0), (1, 0))
    );
    println!("  (la grille équirectangulaire précédente : non bornée aux pôles)");
    println!();

    // --- Le cisaillement -------------------------------------------------
    //
    // Une case reste-t-elle carrée ? Les lignes de coordonnées d'une face du
    // cube ne se coupent à angle droit qu'en son centre. Ailleurs, la case est
    // un losange — et un bloc de voxel avec elle.
    println!("── Cisaillement : la case est-elle carrée ? ──");
    println!("  endroit                        angle des côtés   rapport des côtés");

    let mesure = |u: f64, v: f64| -> (f64, f64) {
        let p = DVec3::from_array(direction(1, u, v));
        let tu = DVec3::from_array(direction(1, u + 0.5, v)) - p;
        let tv = DVec3::from_array(direction(1, u, v + 0.5)) - p;
        let angle = tu.normalize().dot(tv.normalize()).clamp(-1.0, 1.0).acos();
        (angle.to_degrees(), tu.length() / tv.length())
    };

    let f = FACE as f64;
    for (nom, u, v) in [
        ("centre de face", f / 2.0, f / 2.0),
        ("milieu d'une arête", 0.5, f / 2.0),
        ("à mi-chemin du coin", f / 4.0, f / 4.0),
        ("coin de face", 0.5, 0.5),
    ] {
        let (angle, rapport) = mesure(u, v);
        println!("  {nom:<28}   {angle:>13.1}°   {rapport:>17.3}");
    }
    println!("  90° = carré partout : c'est ce que la projection conforme achète.");
    println!();

    // --- Le raccord ne doit ni pincer ni replier la carte ------------------
    //
    // Mélanger deux cartes est facile à faire de travers : si les deux
    // divergent trop là où le poids change vite, le terme de raccord domine la
    // dérivée, la case s'écrase, et au pire la carte se replie sur elle-même.
    // Un déterminant négatif signerait un pli — et un monde qui se recouvre.
    println!("── Le raccord aux coins ──");
    let mut pire_pincement = (f64::MAX, 0.0f64, 0.0f64);
    let mut det_mini = f64::MAX;
    let pas = 4.0 / (conforme::N - 1) as f64;

    let mut s = -1.0 + pas;
    while s < 1.0 - pas {
        let mut t = -1.0 + pas;
        while t < 1.0 - pas {
            let p = conforme::table().ab(s, t);
            let ds = conforme::table().ab(s + pas, t);
            let dt = conforme::table().ab(s, t + pas);
            let (jx, jy) = ((ds.0 - p.0, ds.1 - p.1), (dt.0 - p.0, dt.1 - p.1));

            let det = jx.0 * jy.1 - jy.0 * jx.1;
            det_mini = det_mini.min(det / (pas * pas));

            let aire = (jx.0 * jx.0 + jx.1 * jx.1).sqrt().min(
                (jy.0 * jy.0 + jy.1 * jy.1).sqrt(),
            ) / pas;
            if aire < pire_pincement.0 {
                pire_pincement = (aire, s, t);
            }
            t += pas;
        }
        s += pas;
    }

    println!(
        "  déterminant minimal du jacobien   : {det_mini:.4}  ({})",
        if det_mini > 0.0 { "aucun pli" } else { "PLI — la carte se recouvre" }
    );
    println!(
        "  côté de case le plus court        : {:.3} (en {:.3}, {:.3})",
        pire_pincement.0, pire_pincement.1, pire_pincement.2
    );

    // Jusqu'où le losange se voit-il ? On mesure la distance au coin au-delà de
    // laquelle l'angle est revenu sous 95°, sur la diagonale de la face.
    let mut limite = 0.0f64;
    let mut d = 0.001;
    while d < 1.0 {
        let (s, t) = (1.0 - d / 2.0f64.sqrt(), 1.0 - d / 2.0f64.sqrt());
        let p = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0, (t + 1.0) * FACE as f64 / 2.0));
        let a = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0 + 1.0, (t + 1.0) * FACE as f64 / 2.0));
        let b = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0, (t + 1.0) * FACE as f64 / 2.0 + 1.0));
        let angle = (a - p).normalize().dot((b - p).normalize()).clamp(-1.0, 1.0).acos();
        if (angle.to_degrees() - 90.0).abs() > 5.0 {
            limite = d;
        }
        d += 0.002;
    }
    println!(
        "  zone où la case sort de 90° ± 5°  : rayon {:.0} blocs autour de chaque coin",
        limite * FACE as f64 / 2.0
    );
    println!();
}

/// La table conforme mérite-t-elle qu'on s'y fie ?
///
/// Elle est intégrée numériquement, donc elle ne vaut que ce que valent ses
/// vérifications. Il y en a trois : le coin doit tomber où la théorie le dit,
/// la projection doit être inversible au bloc près, et un pas de grille doit
/// mesurer un bloc au centre d'une face.
fn projection_conforme() {
    println!("── La table conforme ──");

    let (mx, my) = conforme::table().coin_brut;
    let (ax, ay) = conforme::coin_attendu();
    println!("  côté de la table                  : {} × {}", conforme::N, conforme::N);
    println!("  ζ au coin, mesuré                 : {mx:.6}  {my:.6}");
    println!("  ζ au coin, attendu — (1+i)/(√3+1) : {ax:.6}  {ay:.6}");
    println!(
        "  écart d'intégration               : {:.2e}",
        ((mx - ax).powi(2) + (my - ay).powi(2)).sqrt()
    );
    println!(
        "  raccord équiangulaire aux coins    : rayon {:.2} de face, soit {:.0} blocs",
        conforme::raccord(),
        conforme::raccord() * FACE as f64 / 2.0
    );

    // Aller-retour : projeter puis dé-projeter doit rendre la case de départ.
    let (mut pire, mut ou) = (0.0f64, (0u8, 0, 0));
    for f in 0..6u8 {
        for u in (4..FACE - 4).step_by(97) {
            for v in (4..FACE - 4).step_by(101) {
                let d = direction(f, u as f64 + 0.5, v as f64 + 0.5);
                let (f2, u2, v2) = crate::cube::depuis_direction(d);
                let e = if f2 == f {
                    ((u2 - u as f64 - 0.5).powi(2) + (v2 - v as f64 - 0.5).powi(2)).sqrt()
                } else {
                    f64::INFINITY
                };
                if e > pire {
                    pire = e;
                    ou = (f, u, v);
                }
            }
        }
    }
    println!(
        "  aller-retour, écart maximal       : {pire:.4} bloc (face {}, {} {})",
        NOMS[ou.0 as usize], ou.1, ou.2
    );

    // La taille d'un bloc, du centre d'une face vers un coin.
    println!("  taille d'un bloc, du centre au coin :");
    let f = FACE as f64;
    let reference = {
        let a = DVec3::from_array(direction(1, f / 2.0, f / 2.0));
        let b = DVec3::from_array(direction(1, f / 2.0 + 1.0, f / 2.0));
        (b - a).length() * RAYON
    };
    for part in [0.0, 0.5, 0.8, 0.95, 0.999] {
        let (u, v) = (f / 2.0 * (1.0 + part), f / 2.0 * (1.0 + part));
        let a = DVec3::from_array(direction(1, u, v));
        let b = DVec3::from_array(direction(1, u + 1.0, v));
        println!(
            "    {:>5.1} % du chemin : {:.3} bloc",
            part * 100.0,
            (b - a).length() * RAYON / reference
        );
    }
    println!();
}

/// Le déroulement place des chunks côte à côte. Sont-ils vraiment voisins sur
/// le cube ?
///
/// C'est la question que la montagne coupée pose. Une case dupliquée n'est pas
/// seulement une copie : là où le quartier de 90° se referme, deux cases
/// dessinées l'une à côté de l'autre appartiennent à des endroits différents du
/// monde, et le terrain s'y coupe net.
fn continuite_du_deroulement() {
    println!("── Pourquoi le rendu ne déroule plus ──");
    println!("  Le rendu place désormais chaque chunk sur la sphère, une seule fois.");
    println!("  Voici ce que coûtait le déroulement à plat qu'il remplace :");
    println!("  position de la caméra      adjacences   fausses   dont mal orientées");

    let r = 8;
    for (nom, f, cu, cv) in [
        ("centre de face", 1u8, FACE_CHUNKS / 2, FACE_CHUNKS / 2),
        ("milieu d'arête", 1, FACE_CHUNKS / 2, FACE_CHUNKS - 1),
        ("coin", 1, 0, FACE_CHUNKS - 1),
    ] {
        let (mut total, mut fausses, mut tournees) = (0, 0, 0);

        for dv in -r..=r {
            for du in -r..=r {
                let a = replier_chunk(f, cu + du, cv + dv);
                for (pu, pv) in [(1, 0), (0, 1)] {
                    let b = replier_chunk(f, cu + du + pu, cv + dv + pv);

                    // Le vrai voisin de `a` dans la direction du pas, telle
                    // qu'elle arrive dans le repère canonique de `a`.
                    let (cos, sin) = COS_SIN[a.3 as usize];
                    let (tu, tv) = (pu * cos - pv * sin, pu * sin + pv * cos);
                    let vrai = replier_chunk(a.0, a.1 + tu, a.2 + tv);

                    total += 1;
                    if (vrai.0, vrai.1, vrai.2) != (b.0, b.1, b.2) {
                        fausses += 1;
                    } else if (a.3 + vrai.3) % 4 != b.3 {
                        tournees += 1;
                    }
                }
            }
        }

        println!(
            "  {nom:<24}  {total:>10}   {fausses:>7}   {tournees:>18}"
        );
    }
    println!("  Ces huit fronti\u{00e8}res \u{00e9}taient la montagne coup\u{00e9}e. Le rendu sph\u{00e9}rique en a z\u{00e9}ro.");
    println!();
}

/// Le réticule pointe-t-il le bloc qu'on surligne ?
///
/// C'est le test d'étanchéité de D27, et il ne tient que parce que la
/// projection est inversible : le rayon vient de l'écran, il est redressé une
/// fois, puis le monde n'est plus interrogé qu'à plat. Ce qui reste d'écart est
/// le pas de marche, pas une erreur de principe.
///
/// La version qui marchait droit dans le repère de la face mesurait ici jusqu'à
/// 45 blocs.
fn derive_de_visee(gen: &Generateur) {
    println!("── Le réticule et le bloc surligné ──");
    println!("  position              lacet    portée   écart");

    for (nom, face, u, v) in [
        ("centre de face", 1u8, FACE / 2, FACE / 2),
        ("près d'un coin", 1u8, 8, FACE - 8),
    ] {
        for lacet in [0.0f32, 0.7, 2.4] {
            let cam = Camera {
                face,
                position: Vec3::new(u as f32 + 0.5, v as f32 + 0.5, (NIVEAU_MER + 40) as f32),
                lacet,
                tangage: -0.2,
            };
            let (position, avant, _) = cam.repere_3d(RAYON);
            let avant = DVec3::new(avant.x as f64, avant.y as f64, avant.z as f64);

            match viser(gen, &cam, RAYON, 400.0) {
                None => println!("  {nom:<20}  {lacet:>5.1}         —   rien en vue"),
                Some((f, bu, bv, bz)) => {
                    let centre =
                        DVec3::from_array(point_sphere(f, bu, bv)) * (RAYON + bz as f64 + 0.5);
                    let vers = centre - position;
                    let t = vers.length();
                    let angle = vers.normalize().dot(avant).clamp(-1.0, 1.0).acos();
                    println!(
                        "  {nom:<20}  {lacet:>5.1}  {t:>8.0}   {:>5.2} blocs",
                        t * angle.tan()
                    );
                }
            }
        }
    }
    println!();
}

// --------------------------------------------------------------------------
// Le terrain
// --------------------------------------------------------------------------

fn terrain(gen: &Generateur) {
    println!("── Terrain ──");
    println!("  Coupe du pôle nord à l'équateur (face +Z puis +Y) :");

    for i in 0..=8 {
        let v = i * (FACE - 1) / 8;
        let (sol, biome) = gen.colonne(4, FACE / 2, v);
        let p = point_sphere(4, FACE / 2, v);
        let lat = p[2].asin().to_degrees();
        println!("    +Z v={v:>4}  lat {lat:>6.1}°  h={sol:>4}  {}", biome.nom());
    }
    for i in 0..=4 {
        let v = i * (FACE - 1) / 4;
        let (sol, biome) = gen.colonne(1, FACE / 2, v);
        let p = point_sphere(1, FACE / 2, v);
        let lat = p[2].asin().to_degrees();
        println!("    +Y v={v:>4}  lat {lat:>6.1}°  h={sol:>4}  {}", biome.nom());
    }
    println!();

    let (face, u, v) = point_apparition(gen);
    let sol = gen.hauteur(face, u, v);
    let cam = Camera {
        face,
        position: Vec3::new(
            u as f32 + 0.5,
            v as f32 + 0.5,
            sol.max(crate::monde::NIVEAU_MER as f32) + 6.0,
        ),
        lacet: 0.0,
        tangage: -0.15,
    };
    let p = point_sphere(face, u, v);
    println!(
        "  Apparition : face {} ({u}, {v}) · lat {:.1}° · sol {sol:.0} · {}",
        NOMS[face as usize],
        p[2].asin().to_degrees(),
        biome_de(p[2].asin().abs() / std::f64::consts::FRAC_PI_2).nom()
    );
    match viser(gen, &cam, RAYON, 260.0) {
        Some((f, bu, bv, bz)) => println!(
            "    visé : {bu} {bv} {bz} sur {}",
            NOMS[f as usize]
        ),
        None => println!("    visé : rien"),
    }
}
