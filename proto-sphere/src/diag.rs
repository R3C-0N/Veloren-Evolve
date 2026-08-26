//! Diagnostic en console : `proto-sphere --diag`.
//!
//! Le prototype existe pour réfuter D27, pas pour l'illustrer. Une capture
//! d'écran ne dit pas si une couture est continue — des nombres, si. Ce mode
//! mesure ce que l'œil ne peut pas trancher.

use crate::monde::{BLOCS_H, BLOCS_W, Generateur, biome, plier_bloc};
use crate::vue3d::{Camera, viser};
use glam::Vec3;

pub fn executer() {
    let gen = Generateur::nouveau(1);

    println!("monde : {} x {} blocs", BLOCS_W, BLOCS_H);
    println!();

    // --- Continuité est-ouest -------------------------------------------
    println!("Couture est-ouest (y = {}) :", BLOCS_H / 2);
    let y = BLOCS_H / 2;
    for x in BLOCS_W - 3..BLOCS_W + 3 {
        let (wx, _, _) = plier_bloc(x, y);
        println!("  x={:>5} (replié {:>4}) : h = {:.2}", x, wx, gen.hauteur(x, y));
    }
    // Une couture est invisible si le denivele qu'on y franchit ressemble a
    // n'importe quel autre pas. On compare donc, sur toutes les latitudes, le
    // saut a la couture au denivele moyen entre deux blocs voisins.
    let (mut saut_couture, mut somme_ordinaire, mut pire) = (0.0f32, 0.0f32, 0.0f32);
    for yy in 0..BLOCS_H {
        let saut = (gen.hauteur(0, yy) - gen.hauteur(-1, yy)).abs();
        saut_couture += saut;
        pire = pire.max(saut);
        for x in 100..110 {
            somme_ordinaire += (gen.hauteur(x + 1, yy) - gen.hauteur(x, yy)).abs();
        }
    }
    println!(
        "  denivele moyen a la couture : {:.3} bloc · ailleurs : {:.3} bloc · pire cas : {:.3}",
        saut_couture / BLOCS_H as f32,
        somme_ordinaire / (BLOCS_H * 10) as f32,
        pire
    );
    println!();

    // --- Continuité polaire ----------------------------------------------
    println!("Pôle nord (x = {}) :", BLOCS_W / 4);
    let x = BLOCS_W / 4;
    for yy in -3..3 {
        let (wx, wy, plis) = plier_bloc(x, yy);
        println!(
            "  y={:>3} (replié {:>4},{:>3} · {} pli) : h = {:.2}",
            yy,
            wx,
            wy,
            plis,
            gen.hauteur(x, yy)
        );
    }
    let mut saut_max = 0.0f32;
    for xx in 0..BLOCS_W {
        let saut = (gen.hauteur(xx, -1) - gen.hauteur(xx, 0)).abs();
        saut_max = saut_max.max(saut);
    }
    println!("  saut maximal en franchissant le pôle : {:.3} bloc", saut_max);
    println!();

    // --- Le terrain a-t-il la tête de D24 ? -------------------------------
    println!("Coupe en latitude (x = 512) :");
    for i in 0..=16 {
        let yy = i * (BLOCS_H - 1) / 16;
        println!(
            "  y={:>4}  h={:>6.1}  {}",
            yy,
            gen.hauteur(512, yy),
            biome(yy).nom()
        );
    }
    println!();

    // --- Visée au point d'apparition --------------------------------------
    let (ax, ay) = crate::monde::point_apparition(&gen);
    let (sx, sy) = (ax as f32 + 0.5, ay as f32 + 0.5);
    let sol = gen.hauteur(ax, ay);
    let cam = Camera {
        position: Vec3::new(sx, sy, sol.max(crate::monde::NIVEAU_MER as f32) + 6.0),
        lacet: 0.0,
        tangage: -0.15,
    };
    println!(
        "Apparition : x={} y={} sol={:.2} biome={} camera_z={:.2}",
        ax,
        ay,
        sol,
        biome(ay).nom(),
        cam.position.z
    );
    match viser(&gen, &cam, 220.0) {
        Some(b) => {
            let d = ((b[0] as f32 - cam.position.x).powi(2)
                + (b[1] as f32 - cam.position.y).powi(2)
                + (b[2] as f32 - cam.position.z).powi(2))
            .sqrt();
            println!("  visé : {:?} à {:.2} blocs", b, d);
        }
        None => println!("  visé : rien"),
    }
}
