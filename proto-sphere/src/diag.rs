//! Diagnostic en console : `proto-sphere --diag`.
//!
//! Le prototype existe pour réfuter D27, pas pour l'illustrer. Une capture
//! d'écran ne dit pas si un recollement est continu — des nombres, si.
//!
//! Ce fichier tient aussi lieu de garantie pour [`crate::cube`] : les vingt-
//! quatre arêtes n'y sont écrites nulle part, elles se déduisent de six bases
//! 3D. Ce sont ces invariants qui répondent du calcul, pas la relecture.

use crate::cube::{
    COS_SIN, FACE, FACE_CHUNKS, NOMS, RAYON, point_sphere, replier_bloc, replier_chunk,
};
use crate::monde::{Generateur, NIVEAU_MER, TAILLE_CHUNK, biome_de, point_apparition};
use crate::vue3d::{Camera, viser};
use glam::{DVec3, Vec3};
use std::collections::HashSet;

pub fn executer() {
    let gen = Generateur::nouveau(1);

    println!("Patron de cube : 6 faces de {FACE} blocs d'arête");
    println!("tour du monde : {} blocs · rayon {RAYON:.0} blocs", 4 * FACE);
    println!();

    invariants();
    coutures(&gen);
    distorsion();
    continuite_du_deroulement();
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
    println!("  face      bord      dénivelé couture   dénivelé ordinaire   écart angulaire");

    let (mut pire_h, mut pire_a) = (0.0f64, 0.0f64);

    for f in 0..6u8 {
        for (bord, nom) in [(0, "+u"), (1, "−u"), (2, "+v"), (3, "−v")] {
            let (mut somme_couture, mut somme_ordinaire, mut angle_max) = (0.0, 0.0, 0.0f64);
            let n = 96;

            for i in 0..n {
                let w = i * FACE / n;
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
                // sphère, ce que mesure n'importe quel autre pas.
                angle_max = angle_max.max(angle(f, dedans, dehors));
            }

            let (hc, ho) = (somme_couture / n as f64, somme_ordinaire / n as f64);
            let ordinaire = angle(f, (FACE / 2, FACE / 2), (FACE / 2 + 1, FACE / 2));
            let ecart = angle_max / ordinaire;
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
