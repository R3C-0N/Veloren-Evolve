//! Diagnostic en console : `proto-sphere --diag`.
//!
//! Le prototype existe pour réfuter D27, pas pour l'illustrer. Une capture
//! d'écran ne dit pas si un recollement est continu — des nombres, si.
//!
//! Ce fichier tient aussi lieu de garantie pour [`crate::cube`] : les vingt-
//! quatre arêtes n'y sont écrites nulle part, elles se déduisent de six bases
//! 3D. Ce sont ces invariants qui répondent du calcul, pas la relecture.

use crate::cube::{
    COS_SIN, FACE, FACE_CHUNKS, NOMS, point_sphere, replier_bloc, replier_chunk,
};
use crate::monde::{Generateur, TAILLE_CHUNK, biome_de, point_apparition};
use crate::vue3d::{Camera, viser};
use glam::Vec3;
use std::collections::HashSet;

pub fn executer() {
    let gen = Generateur::nouveau(1);

    println!("Patron de cube : 6 faces de {FACE} blocs d'arête");
    println!("tour du monde : {} blocs", 4 * FACE);
    println!();

    invariants();
    coutures(&gen);
    distorsion();
    cout_des_coins();
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

fn cout_des_coins() {
    println!("── Le défaut des huit coins ──");

    // Caméra posée sur le chunk de coin d'une face : combien du champ de vision
    // le déroulement doit-il inventer ?
    for r in [4, 8, 12] {
        let (ccu, ccv) = (0, 0);
        let mut vus = HashSet::new();
        let (mut total, mut doublons) = (0, 0);
        for dv in -r..=r {
            for du in -r..=r {
                let (fc, cu, cv, _) = replier_chunk(1, ccu + du, ccv + dv);
                total += 1;
                if !vus.insert((fc, cu, cv)) {
                    doublons += 1;
                }
            }
        }
        println!(
            "  distance {r:>2} chunks : {doublons} chunks dupliqués sur {total} ({:.1} %)",
            100.0 * doublons as f64 / total as f64
        );
    }
    println!("  (au centre d'une face, aucun : le défaut ne se voit qu'aux coins)");
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
    match viser(gen, &cam, 220.0) {
        Some(b) => {
            let d = ((b[0] as f32 - cam.position.x).powi(2)
                + (b[1] as f32 - cam.position.y).powi(2)
                + (b[2] as f32 - cam.position.z).powi(2))
            .sqrt();
            println!("    visé : {b:?} à {d:.2} blocs");
        }
        None => println!("    visé : rien"),
    }
}
